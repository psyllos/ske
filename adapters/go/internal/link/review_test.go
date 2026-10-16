package link

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"strings"
	"testing"

	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
	cir "ske/adapters/go/gen/cir/v1"
	"ske/adapters/go/internal/decisions"
	"ske/adapters/go/internal/lower"
)

func reviewSource(t *testing.T, source string) ([]decisions.Decision, lower.Result) {
	t.Helper()
	fs := token.NewFileSet()
	file, err := parser.ParseFile(fs, "/project/review.go", source, 0)
	if err != nil {
		t.Fatal(err)
	}
	p, _, err := ssautil.BuildPackage(&types.Config{}, fs, types.NewPackage("example", "p"), []*ast.File{file}, ssa.GlobalDebug)
	if err != nil {
		t.Fatal(err)
	}
	r := lower.Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	var functions []decisions.Function
	for _, fn := range r.Functions {
		if fn.Pkg == p && fn.Syntax() != nil {
			functions = append(functions, decisions.Function{ID: r.FunctionIDs[fn], Node: fn.Syntax()})
		}
	}
	return decisions.Emit(fs, "/project", functions), r
}

func explosion(groups int) (string, string) {
	var params, operands []string
	for i := 0; i < groups; i++ {
		params = append(params, fmt.Sprintf("a%d,b%d", i, i))
		operands = append(operands, fmt.Sprintf("(a%d || b%d)", i, i))
	}
	return strings.Join(params, ",") + " bool", strings.Join(operands, " && ")
}

func TestReviewActivationEnumerationIsBounded(t *testing.T) {
	_, condition := explosion(13) // 8192 true alternatives without the cap.
	expr, err := parser.ParseExpr(condition)
	if err != nil {
		t.Fatal(err)
	}
	if got := len(paths(expr, true)); got > 4096 {
		t.Fatalf("materialized %d alternatives, normative maximum is 4096", got)
	}
}

func TestReviewActivationLimitKeepsInventory(t *testing.T) {
	for _, groups := range []int{12, 13} {
		t.Run(fmt.Sprint(groups), func(t *testing.T) {
			params, condition := explosion(groups)
			ds, lowered := reviewSource(t, "package p\nfunc E("+params+") bool { if "+condition+" { return true }; return false }")
			wantDecisions := 1 + 2*groups
			if len(ds) != wantDecisions {
				t.Fatalf("source inventory lost: %d != %d", len(ds), wantDecisions)
			}
			records := Build(ds, lowered.Branches)
			for _, d := range ds {
				if len(d.Record.Outcomes) != 2 {
					t.Fatal("source outcomes lost")
				}
				if d.Record.DecisionKind != "IF" {
					continue
				}
				for ordinal, outcome := range d.Record.Outcomes {
					cases := 0
					for _, rec := range records {
						if a := rec.GetSourceOutcomeActivationCase(); a != nil && a.SourceOutcomeId == outcome.Id {
							if a.CaseOrdinal != uint32(cases) || len(a.OrderedControlEdgeIds) == 0 {
								t.Fatalf("invalid bounded case: %v", a)
							}
							cases++
						}
					}
					// 2^n true alternatives; 2^n-1 false alternatives.
					want := (1 << groups) - ordinal
					if want > 4096 {
						want = 4096
					}
					if cases != want || outcome.ActivationMappingComplete != (want < 4096) {
						t.Fatalf("cases=%d want=%d complete=%v", cases, want, outcome.ActivationMappingComplete)
					}
				}
			}
		})
	}
}

func TestReviewLazyPathsPreserveShortCircuitTerms(t *testing.T) {
	expr, err := parser.ParseExpr("!(a || b) && (c || d)")
	if err != nil {
		t.Fatal(err)
	}
	got := paths(expr, true)
	want := [][]string{{"a=false", "b=false", "c=true"}, {"a=false", "b=false", "c=false", "d=true"}}
	if len(got) != len(want) {
		t.Fatalf("paths=%v", got)
	}
	for i, path := range got {
		if len(path) != len(want[i]) {
			t.Fatalf("path %d length=%d", i, len(path))
		}
		for j, term := range path {
			if fmt.Sprintf("%s=%v", term.expr.(*ast.Ident).Name, term.value) != want[i][j] {
				t.Fatalf("wrong ordered term at %d/%d: %v", i, j, term)
			}
		}
	}
}

func TestReviewCompoundNegationProvenance(t *testing.T) {
	for _, tc := range []struct {
		condition string
		inverted  bool
	}{{"!(a && b)", true}, {"!(!(a && b))", false}} {
		t.Run(tc.condition, func(t *testing.T) {
			ds, lowered := reviewSource(t, "package p\nfunc N(a,b bool) bool { if "+tc.condition+" { return true }; return false }")
			records := Build(ds, lowered.Branches)
			parent := ""
			for _, d := range ds {
				if d.Record.DecisionKind == "IF" {
					parent = d.Record.Id + "/"
				}
			}
			links := 0
			for _, rec := range records {
				if link := rec.GetSourceIrLink(); link != nil && strings.HasPrefix(link.SourceOutcomeId, parent) {
					links++
					if link.Inverted != tc.inverted || link.Quality != "EXACT" {
						t.Errorf("wrong polarity/quality: %v", link)
					}
				}
			}
			if parent == "" || links != 3 {
				t.Fatalf("expected three parent links, got %d", links)
			}
		})
	}
}

func TestReviewChannelRangeHeaderMapping(t *testing.T) {
	ds, lowered := reviewSource(t, `package p
func C(ch chan int) int {
    n:=0
    for x:=range ch { if x>0 { n+=x }; if _,ok:= <-ch; !ok { break } }
    return n
}`)
	records := Build(ds, lowered.Branches)
	for _, d := range ds {
		if d.Record.DecisionKind != "RANGE_ITERATION" {
			continue
		}
		for i, o := range d.Record.Outcomes {
			if !o.ActivationMappingComplete {
				t.Errorf("range outcome not mapped: %v", o)
			}
			found := false
			for _, rec := range records {
				if a := rec.GetSourceOutcomeActivationCase(); a != nil && a.SourceOutcomeId == o.Id {
					if len(a.OrderedControlEdgeIds) != 1 || a.Quality != "EXACT" {
						t.Fatalf("invalid range activation: %v", a)
					}
					found = true
					for _, branch := range lowered.Branches {
						want := branch.TrueEdgeID
						if i == 1 {
							want = branch.FalseEdgeID
						}
						if a.OrderedControlEdgeIds[0] == want {
							recv, ok := branch.If.Cond.(*ssa.Extract)
							if !ok || recv.Index != 1 {
								t.Fatal("range not bound to the receive ok-result")
							}
							if recv.Tuple.(*ssa.UnOp).Pos() != d.Node.Pos() {
								t.Fatal("matched a body receive instead of the range header")
							}
						}
					}
				}
			}
			if !found {
				t.Error("range case missing")
			}
		}
		return
	}
	t.Fatal("range inventory missing")
}

func TestReviewIncompleteDiagnosticTriplet(t *testing.T) {
	ds := []decisions.Decision{{Record: &cir.SourceDecisionDef{
		Id: "f/sd0", FunctionId: "f", DecisionKind: "SELECT",
		Outcomes: []*cir.SourceOutcomeDef{{Id: "f/sd0/o0", Label: "CASE:0"}},
	}}}
	records := Build(ds, nil)
	if len(records) != 1 {
		t.Fatalf("expected incomplete diagnostic, got %v", records)
	}
	d := records[0].GetDiagnostic()
	if d.GetCode() != "SOURCE_ACTIVATION_INCOMPLETE" || d.Severity != "INFO" || d.Impact != "SYMBOLIC" {
		t.Fatalf("wrong diagnostic triplet: %v", d)
	}
}
