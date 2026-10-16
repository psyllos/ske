// Package lower translates typed Go SSA to the language-neutral CIR stream.
package lower

import (
	"encoding/binary"
	"fmt"
	"go/ast"
	"go/constant"
	"go/token"
	"go/types"
	"math"
	"math/big"
	"path/filepath"
	"sort"
	"strings"

	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
	cir "ske/adapters/go/gen/cir/v1"
	"ske/adapters/go/internal/calls"
)

type Branch struct {
	Function                *ssa.Function
	If                      *ssa.If
	TrueEdgeID, FalseEdgeID string
}
type Result struct {
	Records     []*cir.Record
	Functions   []*ssa.Function
	FunctionIDs map[*ssa.Function]string
	Branches    []Branch
}
type lowerer struct {
	Result
	pkg             *ssa.Package
	fset            *token.FileSet
	root            string
	arch            string
	sizes           types.Sizes
	types           map[string]*cir.TypeDef
	values          map[ssa.Value]string
	globals         map[string]bool
	constants       map[string]string
	resolver        *calls.Resolver
	fn              *ssa.Function
	next, nextConst int
	diagnostics     map[string]bool
	exits           []*cir.ImplicitExitSiteDef
	constPrelude    []*cir.InstructionDef
}

func Lower(pkg *ssa.Package, fset *token.FileSet, root string, sizes types.Sizes) Result {
	return LowerWithArch(pkg, fset, root, sizes, "")
}

// LowerWithArch preserves the build profile's architecture in machine type IDs.
func LowerWithArch(pkg *ssa.Package, fset *token.FileSet, root string, sizes types.Sizes, arch string) Result {
	if sizes == nil {
		sizes = types.SizesFor("gc", "amd64")
	}
	l := &lowerer{pkg: pkg, fset: fset, root: root, sizes: sizes, arch: arch, types: map[string]*cir.TypeDef{}, globals: map[string]bool{}, diagnostics: map[string]bool{}}
	l.FunctionIDs = map[*ssa.Function]string{}
	l.resolver = calls.New(pkg.Prog)
	all := ssautil.AllFunctions(pkg.Prog)
	for _, m := range pkg.Members {
		if f, ok := m.(*ssa.Function); ok {
			all[f] = true
		}
		if declaration, ok := m.(*ssa.Type); ok {
			if named, ok := declaration.Type().(*types.Named); ok {
				for n := 0; n < named.NumMethods(); n++ {
					if function := pkg.Prog.FuncValue(named.Method(n)); function != nil {
						all[function] = true
					}
				}
			}
		}
	}
	for f := range all {
		if f.Pkg == pkg && (f.Syntax() != nil || f.Name() == "init") {
			l.addFunction(f)
		}
	}
	// Discover referenced wrappers, closures, instances and external stubs.
	for n := 0; n < len(l.Functions); n++ {
		f := l.Functions[n]
		if !l.local(f) {
			continue
		}
		for _, a := range f.AnonFuncs {
			l.addFunction(a)
		}
		for _, b := range f.Blocks {
			for _, i := range b.Instrs {
				if c, ok := i.(ssa.CallInstruction); ok {
					if _, builtin := c.Common().Value.(*ssa.Builtin); !builtin {
						for _, t := range l.resolver.Resolve(c).Targets {
							l.addFunction(t)
						}
					}
				}
				for _, o := range i.Operands(nil) {
					if o != nil {
						if f, ok := (*o).(*ssa.Function); ok {
							l.addFunction(f)
						}
					}
				}
			}
		}
	}
	sort.Slice(l.Functions, func(i, j int) bool { return l.FunctionIDs[l.Functions[i]] < l.FunctionIDs[l.Functions[j]] })
	// Inventory declarations even when no instruction references them.
	names := make([]string, 0, len(pkg.Members))
	for name := range pkg.Members {
		names = append(names, name)
	}
	sort.Strings(names)
	l.values = map[ssa.Value]string{}
	for _, name := range names {
		member := pkg.Members[name]
		switch member := member.(type) {
		case *ssa.Global:
			l.value(member)
		case *ssa.Type:
			id := pkg.Pkg.Path() + "." + name
			l.Records = append(l.Records, &cir.Record{Content: &cir.Record_SymbolDef{SymbolDef: &cir.SymbolDef{Id: id, Name: name, QualifiedName: id, Kind: "TYPE", TypeId: l.typ(member.Type())}}})
		case *ssa.NamedConst:
			id := pkg.Pkg.Path() + "." + name
			l.Records = append(l.Records, &cir.Record{Content: &cir.Record_SymbolDef{SymbolDef: &cir.SymbolDef{Id: id, Name: name, QualifiedName: id, Kind: "CONSTANT", TypeId: l.typ(member.Type())}}})
		}
	}
	for _, f := range l.Functions {
		l.function(f)
	}
	keys := make([]string, 0, len(l.types))
	for k := range l.types {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	records := make([]*cir.Record, 0, len(keys)+len(l.Records))
	for _, k := range keys {
		records = append(records, &cir.Record{Content: &cir.Record_TypeDef{TypeDef: l.types[k]}})
	}
	l.Records = append(records, l.Records...)
	return l.Result
}

func (l *lowerer) local(f *ssa.Function) bool {
	if f.Pkg == l.pkg {
		return true
	}
	if object := f.Object(); object != nil && object.Pkg() == l.pkg.Pkg {
		return true
	}
	if receiver := f.Signature.Recv(); receiver != nil {
		t := receiver.Type()
		if pointer, ok := t.(*types.Pointer); ok {
			t = pointer.Elem()
		}
		if named, ok := t.(*types.Named); ok && named.Obj().Pkg() == l.pkg.Pkg {
			return true
		}
	}
	if f.Parent() != nil {
		return l.local(f.Parent())
	}
	if f.Origin() != nil && f.Origin() != f {
		return l.local(f.Origin())
	}
	return false
}
func (l *lowerer) addFunction(f *ssa.Function) {
	if f == nil || l.FunctionIDs[f] != "" {
		return
	}
	l.FunctionIDs[f] = qualified(f)
	// CHA can expose several SSA wrapper objects for the same declared
	// external method (e.g. bound methods discovered in dependencies).
	// They share one CIR symbol/stub, not duplicate declarations.
	for _, existing := range l.Functions {
		if l.FunctionIDs[existing] == l.FunctionIDs[f] {
			return
		}
	}
	l.Functions = append(l.Functions, f)
	if o := f.Origin(); o != nil && o != f {
		l.addFunction(o)
	}
}
func qualified(f *ssa.Function) string {
	if p := f.Parent(); p != nil {
		return qualified(p) + "$" + strings.TrimPrefix(f.Name(), p.Name()+"$")
	}
	p := ""
	if f.Pkg != nil {
		p = f.Pkg.Pkg.Path() + "."
	} else if o := f.Object(); o != nil && o.Pkg() != nil {
		p = o.Pkg().Path() + "."
	}
	receiver := f.Signature.Recv()
	if receiver == nil {
		// Bound-method wrappers drop the receiver from their callable signature,
		// but retain the declared method object.
		if object, ok := f.Object().(*types.Func); ok {
			receiver = object.Type().(*types.Signature).Recv()
		}
	}
	if r := receiver; r != nil {
		t := types.TypeString(r.Type(), func(*types.Package) string { return "" })
		if strings.HasPrefix(t, "*") {
			t = "(" + t + ")"
		}
		return p + t + "." + f.Name()
	}
	return p + f.Name()
}
func (l *lowerer) span(pos token.Pos) *cir.Span {
	if !pos.IsValid() {
		return nil
	}
	end := pos + 1
	if l.fn != nil && l.fn.Syntax() != nil {
		var chosen ast.Expr
		ast.Inspect(l.fn.Syntax(), func(n ast.Node) bool {
			if e, ok := n.(ast.Expr); ok && e.Pos() <= pos && pos < e.End() && (chosen == nil || e.End()-e.Pos() < chosen.End()-chosen.Pos()) {
				chosen = e
			}
			return true
		})
		if chosen != nil {
			pos, end = chosen.Pos(), chosen.End()
		}
	}
	p := l.fset.Position(pos)
	last := l.fset.Position(end)
	path, err := filepath.Rel(l.root, p.Filename)
	if err != nil || path == ".." || strings.HasPrefix(path, ".."+string(filepath.Separator)) {
		return nil
	}
	return &cir.Span{Path: filepath.ToSlash(path), StartLine: uint32(p.Line), StartColumn: uint32(p.Column), EndLine: uint32(last.Line), EndColumn: uint32(last.Column)}
}
func (l *lowerer) diag(code, impact, message string, pos token.Pos) {
	id := l.FunctionIDs[l.fn]
	key := id + "|" + code + "|" + message
	if code == "CONCURRENCY_PARTIAL" || code == "DEFER_UNMODELED" || code == "RECOVER_UNMODELED" {
		key = id + "|" + code
	}
	if l.diagnostics[key] {
		return
	}
	l.diagnostics[key] = true
	severity := "INFO"
	if code == "DEFER_UNMODELED" || code == "CONCURRENCY_PARTIAL" || code == "RECOVER_UNMODELED" {
		severity = "WARNING"
	}
	l.Records = append(l.Records, &cir.Record{Content: &cir.Record_Diagnostic{Diagnostic: &cir.Diagnostic{Code: code, Impact: impact, Severity: severity, Message: message, AffectedFunctionId: id, Span: l.span(pos)}}})
}
func (l *lowerer) function(f *ssa.Function) {
	l.fn = f
	l.values = map[ssa.Value]string{}
	l.constants = map[string]string{}
	l.next = 0
	l.nextConst = 0
	l.exits = nil
	l.constPrelude = nil
	id := l.FunctionIDs[f]
	external := !l.local(f) || len(f.Blocks) == 0
	s := &cir.SymbolDef{Id: id, Name: f.Name(), QualifiedName: id, Kind: "FUNCTION", TypeId: l.typ(f.Signature)}
	if p := l.span(f.Pos()); p != nil && !external {
		s.Spans = []*cir.Span{p}
	}
	l.Records = append(l.Records, &cir.Record{Content: &cir.Record_SymbolDef{SymbolDef: s}})
	d := &cir.FunctionDef{Id: id, SymbolId: id, IsExternal: external}
	for _, p := range f.Params {
		d.ParameterValueIds = append(d.ParameterValueIds, l.value(p))
	}
	for _, p := range f.FreeVars {
		d.ParameterValueIds = append(d.ParameterValueIds, l.value(p))
	}
	for i := 0; i < f.Signature.Results().Len(); i++ {
		d.ReturnTypeIds = append(d.ReturnTypeIds, l.typ(f.Signature.Results().At(i).Type()))
	}
	if !external {
		d.EntryBlockId = fmt.Sprintf("%s/b%d", id, f.Blocks[0].Index)
	}
	l.Records = append(l.Records, &cir.Record{Content: &cir.Record_FunctionDef{FunctionDef: d}})
	if external {
		return
	}
	if f.Synthetic != "" {
		l.diag("SYNTHETIC_FUNCTION", "OTHER", f.Synthetic, f.Pos())
	}
	if f.Origin() == nil && f.TypeParams() != nil && f.TypeParams().Len() > 0 {
		l.diag("GENERIC_ORIGIN_OPAQUE", "TYPES", "generic origin operations are conservative", f.Pos())
	}
	for _, b := range f.Blocks {
		for _, i := range b.Instrs {
			if _, debug := i.(*ssa.DebugRef); debug {
				continue
			}
			if v, ok := i.(ssa.Value); ok {
				if t, ok := v.Type().(*types.Tuple); !ok || t.Len() > 0 {
					l.value(v)
				}
			}
			for _, operand := range i.Operands(nil) {
				if operand != nil && *operand != nil {
					if _, builtin := (*operand).(*ssa.Builtin); !builtin {
						l.value(*operand)
					}
				}
			}
		}
	}
	for _, b := range f.Blocks {
		bid := fmt.Sprintf("%s/b%d", id, b.Index)
		l.Records = append(l.Records, &cir.Record{Content: &cir.Record_BlockDef{BlockDef: &cir.BlockDef{Id: bid, FunctionId: id, Ordinal: uint32(b.Index)}}})
		ordinal := 0
		saved := map[ssa.Value]string{}
		if b == f.Recover && len(b.Preds) == 0 {
			// go/ssa's recovery entry has an implicit environment,
			// not a normal CFG predecessor. CIR must not claim that
			// entry-block SSA definitions dominate it. Materialize
			// unknown local environment values, keeping the block
			// and its real instructions without inventing edges.
			for _, instruction := range b.Instrs {
				if _, debug := instruction.(*ssa.DebugRef); debug {
					continue
				}
				for _, operand := range instruction.Operands(nil) {
					if operand == nil || *operand == nil {
						continue
					}
					v := *operand
					definition, ok := v.(ssa.Instruction)
					if !ok || definition.Block() == b {
						continue
					}
					if _, exists := saved[v]; exists {
						continue
					}
					saved[v] = l.values[v]
					local := fmt.Sprintf("%s/v%d", id, l.next)
					l.next++
					l.values[v] = local
					l.Records = append(l.Records,
						&cir.Record{Content: &cir.Record_ValueDef{ValueDef: &cir.ValueDef{Id: local, FunctionId: id, TypeId: l.typ(v.Type()), Role: "SSA"}}},
						&cir.Record{Content: &cir.Record_InstructionDef{InstructionDef: &cir.InstructionDef{Id: fmt.Sprintf("%s/i%d", bid, ordinal), BlockId: bid, Ordinal: uint32(ordinal), Opcode: "OPAQUE_OP", ResultValueId: local, Payload: &cir.OperationPayload{Details: &cir.OperationPayload_Opaque{Opaque: &cir.OpaqueSemantics{SourceConstruct: "recover environment", Reason: "implicit recovery environment is not modeled"}}}}}})
					ordinal++
				}
			}
			if len(saved) > 0 {
				l.diag("OPAQUE_OP_EMITTED", "SYMBOLIC", "recover environment", f.Pos())
			}
		}
		if b == f.Blocks[0] {
			for _, instruction := range l.constPrelude {
				instruction.Id = fmt.Sprintf("%s/i%d", bid, ordinal)
				instruction.BlockId = bid
				instruction.Ordinal = uint32(ordinal)
				l.Records = append(l.Records, &cir.Record{Content: &cir.Record_InstructionDef{InstructionDef: instruction}})
				ordinal++
			}
		}
		for _, i := range b.Instrs {
			if _, ok := i.(*ssa.DebugRef); ok {
				continue
			}
			ordinal += l.instruction(i, bid, ordinal)
		}
		for v, original := range saved {
			l.values[v] = original
		}
	}
	sort.SliceStable(l.exits, func(i, j int) bool {
		a, b := l.exits[i], l.exits[j]
		if a.Span.GetStartLine() != b.Span.GetStartLine() {
			return a.Span.GetStartLine() < b.Span.GetStartLine()
		}
		if a.Span.GetStartColumn() != b.Span.GetStartColumn() {
			return a.Span.GetStartColumn() < b.Span.GetStartColumn()
		}
		return a.ExitKind < b.ExitKind
	})
	for n, x := range l.exits {
		x.Id = fmt.Sprintf("%s/x%d", id, n)
		l.Records = append(l.Records, &cir.Record{Content: &cir.Record_ImplicitExitSite{ImplicitExitSite: x}})
	}
	if len(l.exits) > 0 {
		l.diag("IMPLICIT_PANIC_UNMODELED", "EFFECTS", fmt.Sprintf("%d implicit panic sites", len(l.exits)), f.Pos())
	}
}
func (l *lowerer) value(v ssa.Value) string {
	if id := l.values[v]; id != "" {
		return id
	}
	d := &cir.ValueDef{TypeId: l.typ(v.Type()), FunctionId: l.FunctionIDs[l.fn], Role: "SSA"}
	switch x := v.(type) {
	case *ssa.Const:
		key := d.TypeId + "|" + x.String()
		kind := l.types[d.TypeId].Kind
		representable := kind == "BITVECTOR" || kind == "BOOL" || kind == "STRING" || kind == "IEEE_FLOAT"
		if x.IsNil() {
			// CIR v1 only specifies null literals for pointer/reference/interface.
			// Preserve other Go nilable types as explicitly opaque SSA constants,
			// never smuggle a null through an untrusted descriptor or fake pointer.
			representable = kind == "POINTER" || kind == "REFERENCE" || kind == "INTERFACE"
		}
		if representable {
			d.Literal = l.literal(x)
			key = fmt.Sprintf("%s|%T|%x|%x|%t|%t", d.TypeId, d.Literal.Representation, d.Literal.GetMachineBitsBe(), d.Literal.GetStringBytes(), d.Literal.GetBoolValue(), d.Literal.GetNullValue())
		}
		if id := l.constants[key]; id != "" {
			return id
		}
		if !representable {
			d.Id = fmt.Sprintf("%s/v%d", d.FunctionId, l.next)
			l.next++
			l.constants[key] = d.Id
			l.constPrelude = append(l.constPrelude, &cir.InstructionDef{Opcode: "OPAQUE_OP", ResultValueId: d.Id, Payload: &cir.OperationPayload{Details: &cir.OperationPayload_Opaque{Opaque: &cir.OpaqueSemantics{SourceConstruct: "constant", Reason: "opaque constant: " + x.String()}}}})
			l.diag("OPAQUE_OP_EMITTED", "SYMBOLIC", "constant", token.NoPos)
			break
		}
		d.Role = "CONST"
		d.Id = fmt.Sprintf("%s/k%d", d.FunctionId, l.nextConst)
		l.nextConst++
		l.constants[key] = d.Id
	case *ssa.Function:
		d.Id = "global:" + qualified(x)
		d.FunctionId = ""
		d.Role = "GLOBAL"
		d.SymbolId = qualified(x)
	case *ssa.Global:
		q := x.Pkg.Pkg.Path() + "." + x.Name()
		d.Id = "global:" + q
		d.FunctionId = ""
		d.Role = "GLOBAL"
		d.SymbolId = q
		if !l.globals[d.Id] {
			l.Records = append(l.Records, &cir.Record{Content: &cir.Record_SymbolDef{SymbolDef: &cir.SymbolDef{Id: q, Name: x.Name(), QualifiedName: q, Kind: "GLOBAL", TypeId: d.TypeId}}})
		}
	case *ssa.Parameter:
		d.Role = "PARAM"
		d.Name = x.Name()
		for n, p := range l.fn.Params {
			if p == x {
				z := uint32(n)
				d.ParamIndex = &z
			}
		}
	case *ssa.FreeVar:
		// Captures are environment parameters, after explicit parameters.
		d.Role = "PARAM"
		d.Name = x.Name()
		for n, p := range l.fn.FreeVars {
			if p == x {
				z := uint32(len(l.fn.Params) + n)
				d.ParamIndex = &z
			}
		}
	}
	if d.Id == "" {
		d.Id = fmt.Sprintf("%s/v%d", d.FunctionId, l.next)
		l.next++
	}
	l.values[v] = d.Id
	if d.Role == "GLOBAL" {
		if l.globals[d.Id] {
			return d.Id
		}
		l.globals[d.Id] = true
	}
	l.Records = append(l.Records, &cir.Record{Content: &cir.Record_ValueDef{ValueDef: d}})
	return d.Id
}
func (l *lowerer) literal(c *ssa.Const) *cir.LiteralValue {
	r := &cir.LiteralValue{}
	if c.IsNil() {
		r.Representation = &cir.LiteralValue_NullValue{NullValue: true}
		return r
	}
	switch c.Value.Kind() {
	case constant.Bool:
		r.Representation = &cir.LiteralValue_BoolValue{BoolValue: constant.BoolVal(c.Value)}
	case constant.String:
		r.Representation = &cir.LiteralValue_StringBytes{StringBytes: []byte(constant.StringVal(c.Value))}
	case constant.Int:
		d := l.types[l.typ(c.Type())]
		n := new(big.Int)
		n.SetString(c.Value.ExactString(), 10)
		mod := new(big.Int).Lsh(big.NewInt(1), uint(d.BitWidth))
		n.Mod(n, mod)
		b := make([]byte, (d.BitWidth+7)/8)
		n.FillBytes(b)
		r.Representation = &cir.LiteralValue_MachineBitsBe{MachineBitsBe: b}
	case constant.Float:
		d := l.types[l.typ(c.Type())]
		b := make([]byte, d.BitWidth/8)
		f, _ := constant.Float64Val(c.Value)
		if len(b) == 4 {
			binary.BigEndian.PutUint32(b, math.Float32bits(float32(f)))
		} else {
			binary.BigEndian.PutUint64(b, math.Float64bits(f))
		}
		r.Representation = &cir.LiteralValue_MachineBitsBe{MachineBitsBe: b}
	default:
		// Complex constants have an opaque type but retain their exact spelling.
		r.Representation = &cir.LiteralValue_StringBytes{StringBytes: []byte(c.Value.ExactString())}
	}
	return r
}
