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
	var env []string
	for _, k := range []string{"PATH", "HOME", "GOCACHE", "GOMODCACHE", "GOROOT"} {
		if v, ok := os.LookupEnv(k); ok {
			env = append(env, k+"="+v)
		}
	}
	work := "off"
	if p.GoWork != nil {
		work = *p.GoWork
	}
	return append(env, "GOENV=off", "GOEXPERIMENT=", "GOFLAGS=-mod=readonly", "GOTOOLCHAIN=local", "GOPROXY=off", "GONOSUMDB=*", "CGO_ENABLED=0", "GOOS="+p.GOOS, "GOARCH="+p.GOARCH, "GOWORK="+work)
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
	config := &packages.Config{Context: ctx, Mode: Mode, Dir: root, Env: Env(p), Fset: fset, Tests: p.IncludeTests}
	if len(p.Tags) > 0 {
		config.BuildFlags = []string{"-tags=" + strings.Join(p.Tags, ",")}
	}
	pkgs, err := packages.Load(config, p.Patterns...)
	if err != nil {
		return nil, err
	}
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
		conf := types.Config{Importer: sourceImporter{pkg.Imports}, Sizes: types.SizesFor("gc", p.GOARCH), GoVersion: "go" + version,
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

type sourceImporter struct{ imports map[string]*packages.Package }

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
