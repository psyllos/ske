package calls

import (
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"testing"

	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
)

func TestStaticAndOpenWorldCHA(t *testing.T) {
	fs := token.NewFileSet()
	file, err := parser.ParseFile(fs, "p.go", `package p
type I interface { M() int }
type T int
func (t T) M() int { return int(t) }
func A() int { return 1 }
func F(i I, f func()int) int { return A()+i.M()+f() }
`, 0)
	if err != nil {
		t.Fatal(err)
	}
	p, _, err := ssautil.BuildPackage(&types.Config{}, fs, types.NewPackage("example", "p"), []*ast.File{file}, ssa.InstantiateGenerics)
	if err != nil {
		t.Fatal(err)
	}
	r := New(p.Prog)
	seen := map[string]bool{}
	for _, block := range p.Func("F").Blocks {
		for _, instruction := range block.Instrs {
			call, ok := instruction.(ssa.CallInstruction)
			if !ok {
				continue
			}
			res := r.Resolve(call)
			seen[res.Dispatch] = true
			if res.Dispatch == "STATIC" {
				if !res.Closed || res.Kind != "STATIC" || len(res.Targets) != 1 || res.Targets[0] != p.Func("A") {
					t.Fatalf("bad static resolution: %+v", res)
				}
			} else if res.Closed || res.Kind != "CHA" {
				t.Fatalf("closed approximate call: %+v", res)
			}
			if res.Dispatch == "INTERFACE" && len(res.Targets) == 0 {
				t.Fatal("missing loaded implementation")
			}
		}
	}
	for _, kind := range []string{"STATIC", "INTERFACE", "DYNAMIC"} {
		if !seen[kind] {
			t.Errorf("missing %s", kind)
		}
	}
}
