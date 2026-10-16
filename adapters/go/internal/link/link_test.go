package link

import (
	"bytes"
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"testing"

	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
	cir "ske/adapters/go/gen/cir/v1"
	"ske/adapters/go/internal/decisions"
	"ske/adapters/go/internal/lower"
)

func TestStructuralMappings(t *testing.T) {
	fs := token.NewFileSet()
	file, err := parser.ParseFile(fs, "/project/p.go", `package p
func R(xs []int) int { n:=0; for _, x := range xs { if x>0 { n++ } }; return n }
func S(a,b chan int) int { select { case x:= <-a: return x; case b <- 1: return 2; default: return 3 } }
func B(a,b chan int) int { select { case <-a: return 1; case <-b: return 2 } }
`, 0)
	if err != nil {
		t.Fatal(err)
	}
	p, _, err := ssautil.BuildPackage(&types.Config{}, fs, types.NewPackage("example", "p"), []*ast.File{file}, ssa.GlobalDebug)
	if err != nil {
		t.Fatal(err)
	}
	result := lower.Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	for _, name := range []string{"R", "S", "B"} {
		fn := p.Func(name)
		var dump bytes.Buffer
		fn.WriteTo(&dump)
		t.Log(dump.String())
		ds := decisions.Emit(fs, "/project", []decisions.Function{{ID: result.FunctionIDs[fn], Node: fn.Syntax()}})
		records := Build(ds, result.Branches)
		for _, record := range records {
			a := record.GetSourceOutcomeActivationCase()
			if a == nil {
				continue
			}
			for _, d := range ds {
				if d.Record.DecisionKind != "SELECT" {
					continue
				}
				for n, o := range d.Record.Outcomes {
					if o.Id != a.SourceOutcomeId {
						continue
					}
					want := n + 1
					if o.Label == "DEFAULT" {
						want = n
					}
					if a.Quality != "EXACT" || len(a.OrderedControlEdgeIds) != want {
						t.Fatalf("select activation prerequisites: %v", a)
					}
				}
			}
		}
		for _, d := range ds {
			for _, o := range d.Record.Outcomes {
				if !o.ActivationMappingComplete {
					t.Errorf("%s %s %s incomplete", name, d.Record.DecisionKind, o.Label)
				}
			}
		}
	}
}

func TestMissingStructuralMappingStaysIncomplete(t *testing.T) {
	d := decisions.Decision{Record: &cir.SourceDecisionDef{Id: "f/sd0", FunctionId: "f", DecisionKind: "SELECT", Outcomes: []*cir.SourceOutcomeDef{{Id: "f/sd0/o0", Label: "CASE:0"}}}}
	records := Build([]decisions.Decision{d}, nil)
	if d.Record.Outcomes[0].ActivationMappingComplete {
		t.Fatal("missing mapping certified")
	}
	if len(records) != 1 || records[0].GetDiagnostic().GetCode() != "SOURCE_ACTIVATION_INCOMPLETE" {
		t.Fatalf("missing explicit diagnostic: %v", records)
	}
}
