package emit

import (
	"bytes"
	"context"
	"golang.org/x/tools/go/packages"
	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
	"os"
	"path/filepath"
	"testing"

	"ske/adapters/go/internal/frame"
	"ske/adapters/go/internal/load"
	"ske/adapters/go/internal/profile"
)

func TestExtractLab(t *testing.T) {
	root, _ := filepath.Abs("../../../../examples/go-branch-lab")
	p, _ := profile.Parse(`{"goos":"linux","goarch":"amd64"}`)
	project, err := load.Open(context.Background(), root, p)
	if err != nil {
		t.Fatal(err)
	}
	for _, pkg := range project.Packages {
		if os.Getenv("SKE_TEST_DUMP_SSA") != "" {
			prog, pkgs := ssautil.AllPackages([]*packages.Package{pkg}, ssa.InstantiateGenerics|ssa.GlobalDebug)
			prog.Build()
			for fn := range ssautil.AllFunctions(prog) {
				if fn.Pkg == pkgs[0] {
					fn.WriteTo(os.Stderr)
				}
			}
		}
		var first bytes.Buffer
		for run := 0; run < 2; run++ {
			records, err := Extract(project, pkg)
			if err != nil {
				t.Fatal(err)
			}
			definitions := map[string]bool{}
			for _, record := range records {
				key := ""
				if d := record.GetSymbolDef(); d != nil {
					key = "symbol:" + d.Id
				}
				if d := record.GetFunctionDef(); d != nil {
					key = "function:" + d.Id
				}
				if d := record.GetCallTargetDef(); d != nil {
					key = "target:" + d.CallInstructionId + "|" + d.TargetFunctionId
				}
				if key != "" && definitions[key] {
					t.Fatalf("duplicate declaration %s", key)
				}
				definitions[key] = true
			}
			var b bytes.Buffer
			for _, r := range records {
				if d := r.GetDiagnostic(); d != nil {
					t.Log(pkg.PkgPath, d.Code, d.AffectedFunctionId, d.Message)
				}
				if err := frame.Write(&b, r, 0); err != nil {
					t.Fatal(err)
				}
			}
			end := records[len(records)-1].GetEndUnit()
			if end == nil || end.InventoryStatus == "FAILED" || end.ExpectedSourceDecisions != end.EmittedSourceDecisions {
				t.Fatalf("end=%v", end)
			}
			t.Log(pkg.PkgPath, end)
			if run == 0 {
				first.Write(b.Bytes())
			} else if !bytes.Equal(first.Bytes(), b.Bytes()) {
				t.Fatal("nondeterministic extraction")
			}
		}
	}
}
