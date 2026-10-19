// Package load loads and typechecks sources without export data or compilation.
package load

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"

	"golang.org/x/tools/go/packages"
	"ske/adapters/go/internal/profile"
)

const Mode = packages.NeedName | packages.NeedFiles | packages.NeedImports | packages.NeedDeps | packages.NeedTypes | packages.NeedSyntax | packages.NeedTypesInfo | packages.NeedModule

type Project struct {
	Root        string
	Profile     profile.Profile
	Version     string
	Fset        *token.FileSet
	Packages    []*packages.Package
	Fingerprint string
}

func Env(p profile.Profile) []string {
	return append(inheritedEnv(), "GOENV=off", "GOEXPERIMENT=", "GOFLAGS=-mod=readonly", "GOTOOLCHAIN=local", "GOPROXY=off", "GONOSUMDB=*", "CGO_ENABLED=0", "GOOS="+p.GOOS, "GOARCH="+p.GOARCH, "GOWORK="+profileWork(p))
}

// inheritedEnv returns the host environment values the adapter deliberately
// preserves, in the same order Env has always used. Nothing else is inherited.
func inheritedEnv() []string {
	var env []string
	for _, k := range []string{"PATH", "HOME", "GOCACHE", "GOMODCACHE", "GOROOT"} {
		if v, ok := os.LookupEnv(k); ok {
			env = append(env, k+"="+v)
		}
	}
	return env
}

func profileWork(p profile.Profile) string {
	work := "off"
	if p.GoWork != nil {
		work = *p.GoWork
	}
	return work
}

// gopathEnv is the closed environment for the GOPATH (GO111MODULE=off) fallback.
// It keeps every lock Env applies but clears GOFLAGS (the -mod flag is module
// only), forces GO111MODULE=off and points GOPATH at the temporary layout.
func gopathEnv(p profile.Profile, gopath string) []string {
	return append(inheritedEnv(), "GOENV=off", "GOEXPERIMENT=", "GOFLAGS=", "GO111MODULE=off", "GOPATH="+gopath, "GOTOOLCHAIN=local", "GOPROXY=off", "GONOSUMDB=*", "CGO_ENABLED=0", "GOOS="+p.GOOS, "GOARCH="+p.GOARCH, "GOWORK="+profileWork(p))
}

// sourceOnlyMode is what the GOPATH fallback loads with: pure go list, no
// parsing/typecheck of the temporary copies. The source-only typecheck below
// then runs once, against the real files, from real paths.
const sourceOnlyMode = packages.NeedName | packages.NeedFiles | packages.NeedImports | packages.NeedDeps

// findGoModUp returns the nearest go.mod found at root or any ancestor
// directory and whether one exists. The walk is lexical (filepath.Dir),
// mirroring go's own upward module search: no directory symlink is resolved
// for the walk itself, only the final go.mod entry is stat'ed.
func findGoModUp(root string) (string, bool) {
	dir := root
	for {
		gomod := filepath.Join(dir, "go.mod")
		if fi, err := os.Stat(gomod); err == nil && !fi.IsDir() {
			return gomod, true
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", false
		}
		dir = parent
	}
}

func Open(ctx context.Context, root string, p profile.Profile) (*Project, error) {
	if !filepath.IsAbs(root) {
		return nil, fmt.Errorf("PROJECT_ROOT_INVALID: project root must be absolute")
	}
	goexe, err := exec.LookPath("go")
	if err != nil {
		return nil, fmt.Errorf("TOOLCHAIN_MISSING: %w", err)
	}
	cmd := exec.CommandContext(ctx, goexe, "version")
	cmd.Env = Env(p)
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("TOOLCHAIN_MISSING: %w", err)
	}
	fields := strings.Fields(string(out))
	if len(fields) < 3 {
		return nil, fmt.Errorf("TOOLCHAIN_MISSING: invalid go version")
	}
	version := strings.TrimPrefix(fields[2], "go")
	if p.GoVersion != nil && strings.TrimPrefix(*p.GoVersion, "go") != version {
		return nil, fmt.Errorf("TOOLCHAIN_MISSING: go version mismatch")
	}
	fset := token.NewFileSet()
	// A project root with no go.mod of its own and an explicit gopath_import
	// profile flag is a pre-module (GOPATH) project: load it through a temporary
	// GOPATH layout instead. Without the flag, module mode is preserved verbatim
	// when any go.mod governs the root, and a gouverned-less root fails with an
	// explicit, actionable error rather than a cryptic go/packages message.
	if p.GopathImport != "" {
		if _, own := os.Stat(filepath.Join(root, "go.mod")); os.IsNotExist(own) {
			return openGOPATH(ctx, root, p, version, fset)
		}
	} else if _, found := findGoModUp(root); !found {
		return nil, fmt.Errorf("PROJECT_MODULE_REQUIRED: %s has no go.mod and is not governed by any module; set profile gopath_import=<import path> to load it as a pre-module (GO111MODULE=off / GOPATH) project", root)
	}
	config := &packages.Config{Context: ctx, Mode: Mode, Dir: root, Env: Env(p), Fset: fset, Tests: p.IncludeTests}
	if len(p.Tags) > 0 {
		config.BuildFlags = []string{"-tags=" + strings.Join(p.Tags, ",")}
	}
	pkgs, err := packages.Load(config, p.Patterns...)
	if err != nil {
		return nil, err
	}
	sourceCheck(fset, version, p.GOARCH, pkgs)
	slices.SortFunc(pkgs, func(a, b *packages.Package) int { return strings.Compare(a.ID, b.ID) })
	metadata := map[string]any{"profile": p, "version": version, "root": root}
	modules := map[string]string{}
	for _, pkg := range pkgs {
		if pkg.Module != nil && pkg.Module.GoMod != "" {
			if b, e := os.ReadFile(pkg.Module.GoMod); e == nil {
				h := sha256.Sum256(b)
				modules[pkg.Module.Path] = hex.EncodeToString(h[:])
			}
		}
	}
	metadata["modules"] = modules
	if p.GoWork != nil && *p.GoWork != "off" {
		path := *p.GoWork
		if !filepath.IsAbs(path) {
			path = filepath.Join(root, path)
		}
		if b, e := os.ReadFile(path); e == nil {
			h := sha256.Sum256(b)
			metadata["work"] = hex.EncodeToString(h[:])
		}
	}
	b, _ := json.Marshal(metadata)
	h := sha256.Sum256(b)
	return &Project{root, p, version, fset, pkgs, hex.EncodeToString(h[:])}, nil
}

// sourceCheck computes, from the real GoFiles, the syntax, types and type info
// that go/packages is asked for but may not always provide. It is the single
// source-only typecheck: no export data, no compilation. Positions are recorded
// in the shared project fset, so spans always reference the real sources.
func sourceCheck(fset *token.FileSet, version, goarch string, pkgs []*packages.Package) {
	// NeedCompiledGoFiles is deliberately absent. go/packages does not in
	// every release parse/typecheck with this mode; explicitly fill from GoFiles
	// using the same source-only importer so no export-data fallback is possible.
	seen := map[*packages.Package]bool{}
	var check func(*packages.Package)
	check = func(pkg *packages.Package) {
		if seen[pkg] {
			return
		}
		seen[pkg] = true
		keys := make([]string, 0, len(pkg.Imports))
		for k := range pkg.Imports {
			keys = append(keys, k)
		}
		slices.Sort(keys)
		for _, k := range keys {
			check(pkg.Imports[k])
		}
		if pkg.Types != nil && pkg.Types.Complete() && len(pkg.Syntax) > 0 {
			return
		}
		pkg.Fset = fset
		pkg.Syntax = nil
		for _, path := range pkg.GoFiles {
			node, e := parser.ParseFile(fset, path, nil, parser.ParseComments|parser.AllErrors)
			if e != nil {
				pkg.Errors = append(pkg.Errors, packages.Error{Msg: e.Error(), Kind: packages.ParseError})
			}
			if node != nil {
				pkg.Syntax = append(pkg.Syntax, node)
			}
		}
		pkg.TypesInfo = &types.Info{
			Types: map[ast.Expr]types.TypeAndValue{}, Defs: map[*ast.Ident]types.Object{},
			Uses: map[*ast.Ident]types.Object{}, Implicits: map[ast.Node]types.Object{},
			Selections: map[*ast.SelectorExpr]*types.Selection{}, Scopes: map[ast.Node]*types.Scope{},
			Instances: map[*ast.Ident]types.Instance{},
		}
		conf := types.Config{Importer: sourceImporter{pkg.Imports}, Sizes: types.SizesFor("gc", goarch), GoVersion: "go" + version,
			Error: func(e error) {
				pkg.Errors = append(pkg.Errors, packages.Error{Msg: e.Error(), Kind: packages.TypeError})
			}}
		if pkg.PkgPath == "unsafe" {
			pkg.Types = types.Unsafe
			pkg.Syntax = nil // compiler intrinsic: no ordinary source declarations
			return
		}
		typed, e := conf.Check(pkg.PkgPath, fset, pkg.Syntax, pkg.TypesInfo)
		pkg.Types = typed
		pkg.IllTyped = e != nil
	}
	for _, pkg := range pkgs {
		check(pkg)
	}
}

type sourceImporter struct{ imports map[string]*packages.Package }

// openGOPATH loads a pre-module project through a temporary GOPATH layout and
// returns a Project whose packages, types, syntax and positions all reference
// the real project files. It is the honest GO111MODULE=off path: the temporary
// copy is only a loading vehicle and is removed before returning.
//
// The import path is the deterministic profile value (gopath_import), so unit
// keys and provenance never depend on the machine, the temp directory, or the
// enclosing module (there is none). pkg.Module is nil in GOPATH mode, so the
// fingerprint carries no module metadata here (see the modules hash in Open).
func openGOPATH(ctx context.Context, root string, p profile.Profile, version string, fset *token.FileSet) (*Project, error) {
	gopath, err := os.MkdirTemp("", "ske-gopath-*")
	if err != nil {
		return nil, fmt.Errorf("PROJECT_GOPATH_TEMP: %w", err)
	}
	defer os.RemoveAll(gopath)
	// src/<import> in GOPATH terms: go resolves the real location, so a physical
	// copy (not a symlink) is required for go to accept it as inside GOPATH.
	src := filepath.Join(gopath, "src", filepath.FromSlash(p.GopathImport))
	if err := os.MkdirAll(filepath.Dir(src), 0o755); err != nil {
		return nil, fmt.Errorf("PROJECT_GOPATH_TEMP: %w", err)
	}
	if err := copyTree(root, src); err != nil {
		return nil, fmt.Errorf("PROJECT_GOPATH_COPY: %w", err)
	}
	config := &packages.Config{Context: ctx, Mode: sourceOnlyMode, Dir: src, Env: gopathEnv(p, gopath), Fset: fset, Tests: p.IncludeTests}
	if len(p.Tags) > 0 {
		config.BuildFlags = []string{"-tags=" + strings.Join(p.Tags, ",")}
	}
	pkgs, err := packages.Load(config, p.Patterns...)
	if err != nil {
		return nil, err
	}
	prefix := filepath.Join(gopath, "src", filepath.FromSlash(p.GopathImport))
	rewrite := func(path string) string { return rewriteGOPATHPrefix(prefix, root, path) }
	// All packages that go/packages produced come from the temporary copy (go
	// list never touches GOROOT for module resolution in GOPATH mode); rewrite
	// every path field back onto the real root, then let sourceCheck parse and
	// typecheck the real files. Clearing types/syntax first guarantees positions
	// never reference the temporary copy.
	for _, pkg := range pkgs {
		pkg.Dir = rewrite(pkg.Dir)
		pkg.GoFiles = mapPaths(pkg.GoFiles, rewrite)
		pkg.CompiledGoFiles = mapPaths(pkg.CompiledGoFiles, rewrite)
		pkg.OtherFiles = mapPaths(pkg.OtherFiles, rewrite)
		pkg.EmbedFiles = mapPaths(pkg.EmbedFiles, rewrite)
		pkg.IgnoredFiles = mapPaths(pkg.IgnoredFiles, rewrite)
		pkg.ExportFile = rewrite(pkg.ExportFile)
		pkg.Types = nil
		pkg.Syntax = nil
		pkg.TypesInfo = nil
	}
	sourceCheck(fset, version, p.GOARCH, pkgs)
	slices.SortFunc(pkgs, func(a, b *packages.Package) int { return strings.Compare(a.ID, b.ID) })
	metadata := map[string]any{"profile": p, "version": version, "root": root, "modules": map[string]string{}}
	if p.GoWork != nil && *p.GoWork != "off" {
		path := *p.GoWork
		if !filepath.IsAbs(path) {
			path = filepath.Join(root, path)
		}
		if b, e := os.ReadFile(path); e == nil {
			h := sha256.Sum256(b)
			metadata["work"] = hex.EncodeToString(h[:])
		}
	}
	b, _ := json.Marshal(metadata)
	h := sha256.Sum256(b)
	return &Project{root, p, version, fset, pkgs, hex.EncodeToString(h[:])}, nil
}

func mapPaths(paths []string, f func(string) string) []string {
	out := make([]string, len(paths))
	for i, s := range paths {
		out[i] = f(s)
	}
	return out
}

// rewriteGOPATHPrefix maps a path from under $gopath/src/<import> back onto the
// real project root; any other path (e.g. GOROOT sources) is returned unchanged.
func rewriteGOPATHPrefix(prefix, root, path string) string {
	rel, err := filepath.Rel(prefix, path)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return path
	}
	return filepath.Join(root, rel)
}

// copyTree copies src into dst, mirroring the project tree under a GOPATH src/
// layout. Go ignores directories named with a leading dot or underscore for
// package matching, so they are skipped (this also excludes .git and VCS data).
// Symlinks are not followed: a symlink inside the project would otherwise pull
// files from outside the project into the load, which the adapter forbids.
func copyTree(src, dst string) error {
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(dst, 0o755); err != nil {
		return err
	}
	for _, e := range entries {
		name := e.Name()
		if strings.HasPrefix(name, ".") || strings.HasPrefix(name, "_") {
			continue
		}
		if e.Type()&os.ModeSymlink != 0 {
			continue
		}
		from, to := filepath.Join(src, name), filepath.Join(dst, name)
		if e.IsDir() {
			if err := copyTree(from, to); err != nil {
				return err
			}
			continue
		}
		b, err := os.ReadFile(from)
		if err != nil {
			return err
		}
		if err := os.WriteFile(to, b, 0o644); err != nil {
			return err
		}
	}
	return nil
}

func (i sourceImporter) Import(path string) (*types.Package, error) {
	if path == "unsafe" {
		return types.Unsafe, nil
	}
	if p := i.imports[path]; p != nil && p.Types != nil {
		return p.Types, nil
	}
	return nil, fmt.Errorf("source dependency unavailable: %s", path)
}

func (p *Project) UnitKey(pkg *packages.Package) string {
	path := pkg.PkgPath
	if strings.Contains(pkg.ID, " [") {
		path += "[test]"
	}
	return "go:" + path + "@" + p.Profile.GOOS + "/" + p.Profile.GOARCH
}
