package emit

import (
	"bytes"
	"context"
	"golang.org/x/tools/go/packages"
	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"ske/adapters/go/internal/frame"
	"ske/adapters/go/internal/load"
	"ske/adapters/go/internal/profile"
)

func TestExtractLegacyGOPATH(t *testing.T) {
	// The full extraction pipeline must work on a pre-module project loaded
	// through the GOPATH fallback: source records, SSA, decisions and links all
	// resolve positions against the real project root, and the stream is
	// deterministic across runs.
	root, _ := filepath.Abs("../../testdata/legacy-project")
	p, _ := profile.Parse(`{"goos":"linux","goarch":"amd64","gopath_import":"legacy.example.com/legacypkg"}`)
	project, err := load.Open(context.Background(), root, p)
	if err != nil {
		t.Fatal(err)
	}
	if len(project.Packages) != 1 {
		t.Fatalf("packages=%d", len(project.Packages))
	}
	pkg := project.Packages[0]
	var first bytes.Buffer
	for run := 0; run < 2; run++ {
		records, err := Extract(project, pkg)
		if err != nil {
			t.Fatal(err)
		}
		for _, r := range records {
			if d := r.GetDiagnostic(); d != nil && d.Impact == "INVENTORY" && d.Severity != "INFO" && d.Severity != "WARNING" {
				t.Log(pkg.PkgPath, d.Code, d.Message)
			}
			if s := r.GetSourceFileDef(); s != nil {
				if strings.HasPrefix(s.Path, "..") || strings.HasPrefix(s.Path, string(filepath.Separator)) {
					t.Fatalf("source path escaped root: %q", s.Path)
				}
			}
		}
		end := records[len(records)-1].GetEndUnit()
		if end == nil || end.InventoryStatus == "FAILED" {
			t.Fatalf("end=%v", end)
		}
		if end.EmittedSourceDecisions != end.ExpectedSourceDecisions {
			t.Fatalf("decisions %d/%d", end.EmittedSourceDecisions, end.ExpectedSourceDecisions)
		}
		var b bytes.Buffer
		for _, r := range records {
			if err := frame.Write(&b, r, 0); err != nil {
				t.Fatal(err)
			}
		}
		if run == 0 {
			first.Write(b.Bytes())
		} else if !bytes.Equal(first.Bytes(), b.Bytes()) {
			t.Fatal("nondeterministic GOPATH extraction")
		}
	}
}

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
