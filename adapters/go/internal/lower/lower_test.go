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
