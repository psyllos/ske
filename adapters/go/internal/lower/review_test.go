package lower

import (
	"go/types"
	"testing"
)

func TestReviewArrayPointerIndexInventoriesNil(t *testing.T) {
	p, fs := sourceSSA(t, `package p
func Read(p *[2]int, i int) int { return p[i] }
func Write(p *[2]int, i int) { p[i] = 1 }
func Address(p *[2]int, i int) *int { return &p[i] }
func Slice(s []int, i int) int { return s[i] }
func Local(i int) int { var a [2]int; return a[i] }
`)
	r := Lower(p, fs, "/project", types.SizesFor("gc", "amd64"))
	for _, name := range []string{"Read", "Write", "Address", "Slice", "Local"} {
		id := r.FunctionIDs[p.Func(name)]
		counts := map[string]int{}
		for _, rec := range r.Records {
			if site := rec.GetImplicitExitSite(); site != nil && site.FunctionId == id {
				counts[site.ExitKind]++
				if site.Span == nil || site.ModelingStatus != "UNMODELED" {
					t.Fatalf("exit without source provenance: %v", site)
				}
			}
		}
		wantNil := 1
		if name == "Slice" || name == "Local" {
			wantNil = 0
		}
		if counts["NIL_DEREFERENCE"] != wantNil || counts["INDEX_OUT_OF_RANGE"] != 1 {
			t.Errorf("%s exits=%v, want nil=%d and one bounds exit", name, counts, wantNil)
		}
	}
}
