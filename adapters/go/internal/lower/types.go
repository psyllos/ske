package lower

import (
	"encoding/binary"
	"go/types"
	"strconv"
	"strings"

	cir "ske/adapters/go/gen/cir/v1"
)

func (l *lowerer) typ(t types.Type) string {
	if t == nil {
		return l.typeRecord(&cir.TypeDef{Id: "go:void", Kind: "VOID"})
	}
	// Go 1.18+ predeclared aliases (`any`, and go/types 1.22+ also materializes
	// `byte`/`rune` this way) are *types.Alias nodes. Collapse them onto the
	// aliased type so `any` and `interface{}` share one type id: TypeString
	// renders the alias as "any" and the explicit spelling as "interface{}",
	// and the CIR validator checks CALL/INVOKE parameters by exact type-id
	// equality, so a mixed boundary (e.g. reflect.ValueOf takes `any` while the
	// caller's field is `interface{}`) would abort ingest with E_TYPE_MISMATCH.
	if alias, ok := t.(*types.Alias); ok {
		return l.typ(types.Unalias(alias))
	}
	if b, ok := t.(*types.Basic); ok && b.Info()&types.IsUntyped != 0 {
		t = types.Default(t)
	}
	var id string
	if b, ok := t.(*types.Basic); ok {
		// Canonicalize Go's byte/rune aliases (byte -> uint8, rune -> int32):
		// go/types represents them as named basic kinds whose TypeString differs
		// from the canonical singleton even though their representation is
		// identical. Leaving them distinct would make comparisons like
		// `path[i] != '/'` (uint8 vs byte) fail CIR type identity checks.
		id = "go:" + types.Typ[b.Kind()].Name()
	} else if sig, ok := t.(*types.Signature); ok {
		// Canonicalize signatures without parameter/result names: function
		// types are equal by types only, but TypeString leaks parameter names,
		// so `func(string) string` (an interface-style signature) and
		// `func(match string) string` (a closure) would get distinct type ids
		// and fail CIR CALL parameter matching (E_TYPE_MISMATCH).
		id = l.signatureID(sig)
	} else {
		id = "go:" + types.TypeString(t, func(p *types.Package) string { return p.Path() })
	}
	if signature, ok := t.(*types.Signature); ok && signature.Recv() == nil {
		// Non-method signatures carry no receiver; nothing to append.
		_ = signature
	}
	if b, ok := t.(*types.Basic); ok && (b.Kind() == types.Int || b.Kind() == types.Uint || b.Kind() == types.Uintptr) {
		arch := l.arch
		if arch == "" {
			arch = strconv.FormatInt(l.sizes.Sizeof(t)*8, 10)
		}
		id += "@" + arch
	}
	if l.types[id] != nil {
		return id
	}
	d := &cir.TypeDef{Id: id, CanonicalDescriptor: id, Kind: "OPAQUE"}
	l.types[id] = d
	switch x := t.Underlying().(type) {
	case *types.Basic:
		switch {
		case x.Info()&types.IsBoolean != 0:
			d.Kind = "BOOL"
		case x.Info()&types.IsInteger != 0:
			d.Kind = "BITVECTOR"
			d.BitWidth = uint32(l.sizes.Sizeof(x) * 8)
			d.IsSigned = x.Info()&types.IsUnsigned == 0
		case x.Info()&types.IsString != 0:
			d.Kind = "STRING"
		case x.Info()&types.IsFloat != 0:
			d.Kind = "IEEE_FLOAT"
			d.BitWidth = uint32(l.sizes.Sizeof(x) * 8)
		}
	case *types.Pointer:
		d.Kind = "POINTER"
		d.Nullable = true
		d.ParamTypeIds = []string{l.typ(x.Elem())}
	case *types.Slice:
		d.Kind = "SLICE"
		d.ParamTypeIds = []string{l.typ(x.Elem())}
	case *types.Array:
		d.Kind = "ARRAY"
		d.ParamTypeIds = []string{l.typ(x.Elem())}
		d.Extra = make([]byte, 8)
		binary.BigEndian.PutUint64(d.Extra, uint64(x.Len()))
	case *types.Struct:
		d.Kind = "STRUCT"
		for i := 0; i < x.NumFields(); i++ {
			d.ParamTypeIds = append(d.ParamTypeIds, l.typ(x.Field(i).Type()))
			d.ComponentNames = append(d.ComponentNames, x.Field(i).Name())
		}
	case *types.Tuple:
		d.Kind = "TUPLE"
		for i := 0; i < x.Len(); i++ {
			d.ParamTypeIds = append(d.ParamTypeIds, l.typ(x.At(i).Type()))
		}
	case *types.Signature:
		d.Kind = "FUNCTION"
		if x.Recv() != nil {
			d.ParamTypeIds = append(d.ParamTypeIds, l.typ(x.Recv().Type()))
		}
		for i := 0; i < x.Params().Len(); i++ {
			d.ParamTypeIds = append(d.ParamTypeIds, l.typ(x.Params().At(i).Type()))
		}
		for i := 0; i < x.Results().Len(); i++ {
			d.ResultTypeIds = append(d.ResultTypeIds, l.typ(x.Results().At(i).Type()))
		}
	case *types.Interface:
		d.Kind = "INTERFACE"
		for i := 0; i < x.NumMethods(); i++ {
			d.ComponentNames = append(d.ComponentNames, x.Method(i).FullName())
		}
	case *types.Map, *types.Chan:
		// Keep nominal IDs, but describe the underlying nilable Go kind
		// so CIR consumers can distinguish it from other opaque types.
		d.CanonicalDescriptor = "go:" + types.TypeString(t.Underlying(), func(p *types.Package) string { return p.Path() })
	}
	return id
}

// signatureID builds a canonical type id for a *types.Signature from its
// receiver/parameter/result types only, never from TypeString (which would leak
// parameter names and make equal function types unequal, e.g. the interface
// parameter `func(string) string` vs the closure `func(match string) string`).
func (l *lowerer) signatureID(sig *types.Signature) string {
	var b strings.Builder
	b.WriteString("go:func")
	if sig.Recv() != nil {
		b.WriteString(":recv=" + l.typ(sig.Recv().Type()))
	}
	b.WriteString("(")
	for i := 0; i < sig.Params().Len(); i++ {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(l.typ(sig.Params().At(i).Type()))
	}
	b.WriteString(")")
	if sig.Results().Len() == 1 {
		b.WriteString(" " + l.typ(sig.Results().At(0).Type()))
	} else if sig.Results().Len() > 1 {
		b.WriteString(" (")
		for i := 0; i < sig.Results().Len(); i++ {
			if i > 0 {
				b.WriteString(", ")
			}
			b.WriteString(l.typ(sig.Results().At(i).Type()))
		}
		b.WriteString(")")
	}
	return b.String()
}

func (l *lowerer) typeRecord(d *cir.TypeDef) string { l.types[d.Id] = d; return d.Id }
