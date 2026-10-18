package lower

import (
	"bytes"
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"strings"
	"testing"

	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
	"google.golang.org/protobuf/proto"
	cir "ske/adapters/go/gen/cir/v1"
)

func TestNilablesAndStrictCallSignatures(t *testing.T) {
	p, fs := sourceSSA(t, `package p
type M map[string]int
type C chan<- int
type T int
func (t T) Method(x int) (int,bool) { return x,true }
func Plain(x int) int { return x }
func Nil() ([]int,func(),M,C,chan int,<-chan int,*T,interface{}) { return nil,nil,nil,nil,nil,nil,nil,nil }
func F(t T) (int,bool) { defer Plain(2); return t.Method(3) }
func Closure(x int) func()int { return func()int { return x } }
`)
	r := Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	ts := map[string]*cir.TypeDef{}
	vs := map[string]*cir.ValueDef{}
	fsigs := map[string]string{}
	for _, rec := range r.Records {
		if d := rec.GetTypeDef(); d != nil {
			ts[d.Id] = d
		}
		if d := rec.GetValueDef(); d != nil {
			vs[d.Id] = d
		}
		if d := rec.GetSymbolDef(); d != nil {
			fsigs[d.Id] = d.TypeId
		}
	}
	nils := 0
	for _, v := range vs {
		if v.Literal == nil || !v.Literal.GetNullValue() {
			continue
		}
		nils++
		ty := ts[v.TypeId]
		if ty.Kind != "POINTER" && ty.Kind != "INTERFACE" {
			t.Fatalf("literal null outside CIR core contract: %v", ty)
		}
	}
	if nils != 2 {
		t.Fatalf("nil count=%d", nils)
	}
	opaqueNils := 0
	for _, rec := range r.Records {
		if i := rec.GetInstructionDef(); i != nil && i.Opcode == "OPAQUE_OP" && strings.HasPrefix(i.Id, "example.Nil/") {
			v := vs[i.ResultValueId]
			kind := ts[v.TypeId].Kind
			if v.Role != "SSA" || v.Literal != nil || (kind != "SLICE" && kind != "FUNCTION" && kind != "OPAQUE") {
				t.Fatalf("nil type was replaced or invented: %v / %v", v, ts[v.TypeId])
			}
			opaqueNils++
		}
	}
	if opaqueNils != 6 {
		t.Fatalf("expected six explicitly opaque nil values, got %d", opaqueNils)
	}
	if fsigs["example.Plain"] == fsigs["example.T.Method"] {
		t.Fatal("receiver signature collision")
	}
	for _, rec := range r.Records {
		i := rec.GetInstructionDef()
		if i == nil || (i.Opcode != "CALL" && i.Opcode != "DEFER_PUSH") {
			continue
		}
		sig := ts[vs[i.OperandValueIds[0]].TypeId]
		if len(sig.ParamTypeIds) != len(i.OperandValueIds)-1 {
			t.Fatalf("call arity: %v signature %v", i, sig)
		}
		for n, arg := range i.OperandValueIds[1:] {
			if vs[arg].TypeId != sig.ParamTypeIds[n] {
				t.Fatalf("call parameter mismatch: %v", i)
			}
		}
		if i.Opcode == "CALL" && len(sig.ResultTypeIds) == 2 {
			result := ts[vs[i.ResultValueId].TypeId]
			if result.Kind != "TUPLE" || len(result.ParamTypeIds) != 2 {
				t.Fatal("multiple return tuple lost")
			}
		}
	}
}

func TestRecoveryEntryHasLocalOpaqueEnvironment(t *testing.T) {
	p, fs := sourceSSA(t, `package p
func F(f func()) (x int) { defer func(){ recover(); x=1 }(); f(); return }
`)
	fn := p.Func("F")
	if fn.Recover == nil || len(fn.Recover.Preds) != 0 {
		t.Fatal("expected independent SSA recovery entry")
	}
	r := Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	block := r.FunctionIDs[fn] + "/b1"
	defs := map[string]bool{}
	found := false
	for _, rec := range r.Records {
		i := rec.GetInstructionDef()
		if i == nil || i.BlockId != block {
			continue
		}
		if i.Opcode == "OPAQUE_OP" && i.Payload.GetOpaque().GetSourceConstruct() == "recover environment" {
			found = true
		}
		for _, arg := range i.OperandValueIds {
			if !defs[arg] {
				t.Fatalf("nonlocal recovery SSA dependency: %v", i)
			}
		}
		if i.ResultValueId != "" {
			defs[i.ResultValueId] = true
		}
	}
	if !found {
		t.Fatal("missing conservative environment materialization")
	}
}

func sourceSSA(t *testing.T, text string) (*ssa.Package, *token.FileSet) {
	t.Helper()
	fs := token.NewFileSet()
	f, err := parser.ParseFile(fs, "/project/p.go", text, 0)
	if err != nil {
		t.Fatal(err)
	}
	p, _, err := ssautil.BuildPackage(&types.Config{}, fs, types.NewPackage("example", "p"), []*ast.File{f}, ssa.InstantiateGenerics|ssa.GlobalDebug)
	if err != nil {
		t.Fatal(err)
	}
	return p, fs
}

// TestByteRuneAliasTypeIdentity guards the byte->uint8 / rune->int32
// canonicalization in typ(). Real Go code regularly compares a string index
// (uint8) against a byte/rune literal (a named basic kind that go/types
// TypeStrings as "byte" / "rune"). Without canonical ids the adapter emits
// two distinct type records of identical representation and the CIR core
// rejects the comparison with E_TYPE_MISMATCH (operand types differ).
func TestByteRuneAliasTypeIdentity(t *testing.T) {
	p, fs := sourceSSA(t, `package p
func F(s string) bool {
	return s[0] != 'a' && s[1] != '\\'
}
func G(rs []rune) bool {
	return rs[0] == 'x'
}
`)
	r := Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	typesByID := map[string]*cir.TypeDef{}
	values := map[string]*cir.ValueDef{}
	for _, rec := range r.Records {
		if d := rec.GetTypeDef(); d != nil {
			typesByID[d.Id] = d
		}
		if d := rec.GetValueDef(); d != nil {
			values[d.Id] = d
		}
	}
	// No `byte`/`rune` records may survive: aliases must collapse onto
	// their canonical basic type id, and there must not be two distinct
	// 8-bit / 32-bit records for the same representation.
	byteWide := 0
	runeWide := 0
	for id, td := range typesByID {
		if td.Kind != "BITVECTOR" {
			continue
		}
		if id == "go:byte" {
			t.Fatalf("byte alias type survived canonicalization: %s", id)
		}
		if id == "go:rune" {
			t.Fatalf("rune alias type survived canonicalization: %s", id)
		}
	}
	for _, td := range typesByID {
		if td.Kind != "BITVECTOR" {
			continue
		}
		switch td.BitWidth {
		case 8:
			byteWide++
		case 32:
			runeWide++
		}
	}
	if byteWide != 1 {
		t.Fatalf("expected exactly one 8-bit BITVECTOR record, got %d", byteWide)
	}
	if runeWide != 1 {
		t.Fatalf("expected exactly one 32-bit BITVECTOR record, got %d", runeWide)
	}
	// Every EQ/NE must have identical operand type ids and a BOOL result.
	for _, rec := range r.Records {
		i := rec.GetInstructionDef()
		if i == nil || (i.Opcode != "EQ" && i.Opcode != "NE") {
			continue
		}
		if len(i.OperandValueIds) != 2 {
			t.Fatalf("EQ/NE arity: %v", i)
		}
		x := values[i.OperandValueIds[0]]
		y := values[i.OperandValueIds[1]]
		if x == nil || y == nil || x.TypeId != y.TypeId {
			t.Fatalf("EQ/NE operand type mismatch: %v (%s vs %s)", i.Id, x.TypeId, y.TypeId)
		}
		if typesByID[x.TypeId].Kind != "BITVECTOR" {
			t.Fatalf("EQ/NE operand not BITVECTOR: %v", i.Id)
		}
	}
}

// TestSignatureTypeNamesDoNotLeak guards the signature canonicalization in
// signatureID(). Real code passes a closure to a method taking a func()-typed
// parameter (godotenv.expandEscapes does `re.ReplaceAllStringFunc(s, func(match
// string) string {...})`): go/types renders the interface-style parameter
// `func(string) string` and the closure `func(match string) string` with
// different TypeString spellings. Without canonical signature ids the adapter
// emits distinct type records for equal function types and the CIR core rejects
// the CALL (E_TYPE_MISMATCH: parameter ids differ).
func TestSignatureTypeNamesDoNotLeak(t *testing.T) {
	p, fs := sourceSSA(t, `package p
type R struct{}
func (r *R) ReplaceAllStringFunc(src string, repl func(string) string) string { return src }
func expandEscapes(str string, re *R) string {
	out := re.ReplaceAllStringFunc(str, func(match string) string {
		return match
	})
	return out
}`)
	r := Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	typesByID := map[string]*cir.TypeDef{}
	values := map[string]*cir.ValueDef{}
	for _, rec := range r.Records {
		if d := rec.GetTypeDef(); d != nil {
			typesByID[d.Id] = d
		}
		if d := rec.GetValueDef(); d != nil {
			values[d.Id] = d
		}
	}
	// No signature id may embed a parameter name (e.g. `func(match string) string`).
	for id := range typesByID {
		if id == "go:byte" || id == "go:rune" {
			t.Fatalf("alias basic type survived canonicalization: %s", id)
		}
	}
	// The CALL to ReplaceAllStringFunc must have operands whose types match the
	// callee signature's parameter types exactly (the closure type must equal the
	// interface-form param type). This is the invariant the real godotenv input
	// violated before signatureID existed.
	for _, rec := range r.Records {
		i := rec.GetInstructionDef()
		if i == nil || i.Opcode != "CALL" {
			continue
		}
		if !strings.Contains(i.Id, "expandEscapes") {
			continue
		}
		callee := values[i.OperandValueIds[0]]
		calleeType := typesByID[callee.TypeId]
		if calleeType.Kind != "FUNCTION" {
			t.Fatalf("CALL callee not FUNCTION: %v", callee)
		}
		for n, param := range calleeType.ParamTypeIds {
			arg := values[i.OperandValueIds[1+n]]
			if arg.TypeId != param {
				t.Fatalf("CALL param %d mismatch: arg %s (type %s) != signature %s", n, arg.Id, arg.TypeId, param)
			}
		}
	}
}

func TestRealSSA(t *testing.T) {
	p, fs := sourceSSA(t, `package p
func helper(x int) int { return x+1 }
func F(x int, p *int) (int,int) { y:=x &^ 3; if y>0 { y=helper(y) }; return y, *p/x }
func Closure(x int) func() int { return func() int { return x } }
func Generic[T ~int](x T) T { return x+1 }
func Instance() int { return Generic(2) }
`)
	r := Lower(p, fs, "/project", types.SizesFor("gc", "386"))
	var first []byte
	ops := map[string]int{}
	defs := map[string]bool{}
	results := map[string]int{}
	for _, rec := range r.Records {
		b, err := proto.MarshalOptions{Deterministic: true}.Marshal(rec)
		if err != nil {
			t.Fatal(err)
		}
		first = append(first, b...)
		if d := rec.GetValueDef(); d != nil {
			if defs[d.Id] {
				t.Fatalf("duplicate value %s", d.Id)
			}
			defs[d.Id] = true
		}
		if d := rec.GetInstructionDef(); d != nil {
			ops[d.Opcode]++
			if d.ResultValueId != "" {
				results[d.ResultValueId]++
			}
		}
		if d := rec.GetTypeDef(); d != nil && d.Id == "go:int@32" && d.BitWidth != 32 {
			t.Fatal("wrong machine width")
		}
	}
	for _, op := range []string{"BIT_NOT", "BIT_AND", "BRANCH", "PHI", "CALL", "DIV", "LOAD", "MAKE_CLOSURE"} {
		if ops[op] == 0 {
			t.Errorf("missing %s", op)
		}
	}
	for _, rec := range r.Records {
		if d := rec.GetValueDef(); d != nil && d.Role == "SSA" && results[d.Id] != 1 {
			t.Errorf("%s has %d definitions", d.Id, results[d.Id])
		}
	}
	for _, rec := range r.Records {
		if d := rec.GetFunctionDef(); d != nil {
			count := 0
			for _, rec := range r.Records {
				if v := rec.GetValueDef(); v != nil && v.FunctionId == d.Id && v.Role == "PARAM" {
					count++
				}
			}
			if count != len(d.ParameterValueIds) {
				t.Errorf("%s parameter list does not include all captures", d.Id)
			}
		}
	}
	for n := 0; n < 5; n++ {
		var again []byte
		for _, rec := range Lower(p, fs, "/project", types.SizesFor("gc", "386")).Records {
			b, _ := proto.MarshalOptions{Deterministic: true}.Marshal(rec)
			again = append(again, b...)
		}
		if !bytes.Equal(first, again) {
			t.Fatal("nondeterministic stream")
		}
	}
	if len(r.Branches) < 2 {
		t.Fatal("init and source branch absent")
	}
}

func TestDynamicCallAndImplicitInventory(t *testing.T) {
	p, fs := sourceSSA(t, `package p
func A(x int) int { return x }
func F(f func(int)int,x int,s []int,m map[int]int) int { m[x]=s[x]; return f(x) }
`)
	r := Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	sites := map[string]bool{}
	dynamic := false
	for _, rec := range r.Records {
		if d := rec.GetImplicitExitSite(); d != nil {
			sites[d.ExitKind] = true
		}
		if d := rec.GetInstructionDef(); d != nil && d.Opcode == "CALL" {
			c := d.Payload.GetCall()
			if c.DispatchKind == "DYNAMIC" {
				dynamic = true
				if c.WorldClosed || c.MayRaise != nil || c.MayMutateMemory != nil {
					t.Fatal("unsound call claims")
				}
			}
		}
	}
	if !dynamic || !sites["INDEX_OUT_OF_RANGE"] || !sites["NIL_MAP_WRITE"] {
		t.Fatalf("missing dynamic call or exits: %v", sites)
	}
}

func TestArchitectureAndOpaqueConstants(t *testing.T) {
	p, fs := sourceSSA(t, `package p
var Unused int
type Node struct { Next *Node }
func Complex() complex128 { return 1+2i }
func Nil() []int { return nil }
`)
	r := LowerWithArch(p, fs, "/project", types.SizesFor("gc", "arm64"), "arm64")
	arch, complex, unused := false, false, false
	for _, rec := range r.Records {
		if d := rec.GetTypeDef(); d != nil && d.Id == "go:int@arm64" {
			arch = d.BitWidth == 64
		}
		if d := rec.GetValueDef(); d != nil {
			if d.Id == "global:example.Unused" {
				unused = true
			}
			if d.Role == "CONST" && d.TypeId == "go:complex128" {
				t.Fatal("invalid opaque literal")
			}
		}
		if d := rec.GetInstructionDef(); d != nil && d.Opcode == "OPAQUE_OP" && d.Payload.GetOpaque().GetSourceConstruct() == "constant" {
			complex = true
		}
	}
	if !arch || !complex || !unused {
		t.Fatalf("architecture=%v opaque constant=%v unused global=%v", arch, complex, unused)
	}
}

func TestBoundWrappersHaveQualifiedIdentity(t *testing.T) {
	p, fs := sourceSSA(t, `package p
type A int
type B int
func (a A) M() int { return int(a) }
func (b B) M() int { return int(b) }
func F(a A,b B) (func()int,func()int) { return a.M,b.M }
`)
	r := Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	defs := map[string]bool{}
	for _, rec := range r.Records {
		if d := rec.GetFunctionDef(); d != nil {
			if defs[d.Id] {
				t.Fatalf("duplicate function %s", d.Id)
			}
			defs[d.Id] = true
			if (d.Id == "example.A.M$bound" || d.Id == "example.B.M$bound") && d.IsExternal {
				t.Fatal("local wrapper emitted as stub")
			}
		}
	}
	if !defs["example.A.M$bound"] || !defs["example.B.M$bound"] {
		t.Fatalf("missing wrappers: %v", defs)
	}
}
