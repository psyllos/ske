// Package emit assembles one deterministic CIR unit from real source and SSA.
package emit

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"go/types"
	"os"
	"path/filepath"
	"runtime/debug"
	"slices"
	"strings"

	"golang.org/x/tools/go/packages"
	"golang.org/x/tools/go/ssa"
	"golang.org/x/tools/go/ssa/ssautil"
	cir "ske/adapters/go/gen/cir/v1"
	"ske/adapters/go/internal/decisions"
	"ske/adapters/go/internal/link"
	"ske/adapters/go/internal/load"
	"ske/adapters/go/internal/lower"
)

func Sources(root string, pkg *packages.Package) ([]*cir.Record, error) {
	paths := slices.Clone(pkg.GoFiles)
	slices.Sort(paths)
	var out []*cir.Record
	for _, path := range paths {
		rel, err := filepath.Rel(root, path)
		if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			return nil, fmt.Errorf("source outside project: %s", path)
		}
		b, err := os.ReadFile(path)
		if err != nil {
			return nil, err
		}
		h := sha256.Sum256(b)
		out = append(out, &cir.Record{Content: &cir.Record_SourceFileDef{SourceFileDef: &cir.SourceFileDef{Path: filepath.ToSlash(rel), Language: "go", ContentSha256: hex.EncodeToString(h[:])}}})
	}
	return out, nil
}

func Extract(p *load.Project, pkg *packages.Package) (records []*cir.Record, err error) {
	// An SSA builder panic is an explicit failed unit, never a truncated success.
	defer func() {
		if r := recover(); r != nil {
			records = Failed(p, pkg, fmt.Sprint(r)+"\n"+string(debug.Stack()))
			err = nil
		}
	}()
	key := p.UnitKey(pkg)
	records = []*cir.Record{
		{Content: &cir.Record_Header{Header: &cir.Header{CirMajor: 1, CirMinor: 3, AdapterName: "ske-adapter-go", AdapterVersion: "0.2.0", Language: "go", ToolchainVersion: p.Version, BuildFingerprint: p.Fingerprint, SemanticProfile: "go-" + p.Version + "-" + p.Profile.GOOS + "-" + p.Profile.GOARCH, CapabilityFlags: []string{"SOURCE_AST", "SSA_CFG", "SOURCE_ACTIVATION", "IMPLICIT_EXIT_INVENTORY"}}}},
		{Content: &cir.Record_UnitDef{UnitDef: &cir.UnitDef{UnitId: key, UnitKey: key, Language: "go"}}},
	}
	sources, e := Sources(p.Root, pkg)
	if e != nil {
		return nil, e
	}
	records = append(records, sources...)
	ignored := slices.Clone(pkg.IgnoredFiles)
	slices.Sort(ignored)
	for _, path := range ignored {
		if !strings.HasSuffix(path, ".go") {
			continue
		}
		rel, e := filepath.Rel(p.Root, path)
		if e != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			continue
		}
		records = append(records, &cir.Record{Content: &cir.Record_Diagnostic{Diagnostic: &cir.Diagnostic{Code: "BUILD_TAG_EXCLUDED_FILE", Severity: "INFO", Impact: "INVENTORY", Message: filepath.ToSlash(rel)}}})
	}
	if pkg.IllTyped || len(pkg.Errors) > 0 {
		var msgs []string
		for _, e := range pkg.Errors {
			msgs = append(msgs, e.Error())
		}
		return Failed(p, pkg, strings.Join(msgs, "\n")), nil
	}
	prog, initial := ssautil.AllPackages([]*packages.Package{pkg}, ssa.InstantiateGenerics|ssa.GlobalDebug)
	prog.Build()
	if len(initial) != 1 || initial[0] == nil {
		return Failed(p, pkg, "no SSA package"), nil
	}
	sp := initial[0]
	lowered := lower.LowerWithArch(sp, p.Fset, p.Root, types.SizesFor("gc", p.Profile.GOARCH), p.Profile.GOARCH)
	records = append(records, lowered.Records...)
	var functions []decisions.Function
	for _, fn := range lowered.Functions {
		if (fn.Pkg == sp || (fn.Origin() != nil && fn.Origin().Pkg == sp)) && fn.Syntax() != nil && len(fn.Blocks) > 0 {
			functions = append(functions, decisions.Function{ID: lowered.FunctionIDs[fn], Node: fn.Syntax()})
		}
	}
	ds := decisions.Emit(p.Fset, p.Root, functions)
	for _, d := range ds {
		records = append(records, &cir.Record{Content: &cir.Record_SourceDecisionDef{SourceDecisionDef: d.Record}})
	}
	// Build per function, as generic instances share AST nodes but not IR IDs.
	for _, fn := range functions {
		var own []decisions.Decision
		var branches []lower.Branch
		for _, d := range ds {
			if d.Record.FunctionId == fn.ID {
				own = append(own, d)
			}
		}
		for _, b := range lowered.Branches {
			if lowered.FunctionIDs[b.Function] == fn.ID {
				branches = append(branches, b)
			}
		}
		records = append(records, link.Build(own, branches)...)
	}
	status := "COMPLETE"
	var edges uint64
	for _, r := range records {
		if r.GetControlEdgeDef() != nil {
			edges++
		}
		if d := r.GetDiagnostic(); d != nil && d.Severity != "INFO" && d.Impact == "INVENTORY" {
			status = "PARTIAL"
		}
	}
	records = append(records, &cir.Record{Content: &cir.Record_EndUnit{EndUnit: &cir.EndUnit{UnitId: key, InventoryStatus: status, ExpectedSourceDecisions: decisions.Count(functions), EmittedSourceDecisions: uint64(len(ds)), EmittedIrEdges: edges}}})
	return records, nil
}

func Failed(p *load.Project, pkg *packages.Package, message string) []*cir.Record {
	key := p.UnitKey(pkg)
	return []*cir.Record{
		{Content: &cir.Record_Header{Header: &cir.Header{CirMajor: 1, CirMinor: 3, AdapterName: "ske-adapter-go", AdapterVersion: "0.2.0", Language: "go", ToolchainVersion: p.Version, BuildFingerprint: p.Fingerprint, SemanticProfile: "go-" + p.Version + "-" + p.Profile.GOOS + "-" + p.Profile.GOARCH}}},
		{Content: &cir.Record_UnitDef{UnitDef: &cir.UnitDef{UnitId: key, UnitKey: key, Language: "go"}}},
		{Content: &cir.Record_Diagnostic{Diagnostic: &cir.Diagnostic{Severity: "ERROR", Code: "UNIT_FAILED", Message: message, Impact: "INVENTORY"}}},
		{Content: &cir.Record_EndUnit{EndUnit: &cir.EndUnit{UnitId: key, InventoryStatus: "FAILED"}}},
	}
}
