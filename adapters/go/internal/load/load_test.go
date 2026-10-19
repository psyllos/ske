package load

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"ske/adapters/go/internal/profile"
)

func TestEnvLocked(t *testing.T) {
	t.Setenv("GOPROXY", "https://invalid")
	t.Setenv("GOFLAGS", "-mod=mod")
	t.Setenv("GOPACKAGESDRIVER", "malicious")
	p, _ := profile.Parse(`{"goos":"linux","goarch":"amd64"}`)
	env := strings.Join(Env(p), "\n")
	for _, want := range []string{"GOFLAGS=-mod=readonly", "GOTOOLCHAIN=local", "GOPROXY=off", "CGO_ENABLED=0", "GOENV=off", "GOEXPERIMENT="} {
		if !strings.Contains(env, want) {
			t.Fatal(want, env)
		}
	}
	if strings.Contains(env, "malicious") || strings.Contains(env, "https://invalid") {
		t.Fatal(env)
	}
}

func TestLoadLab(t *testing.T) {
	root, err := filepath.Abs("../../../../examples/go-branch-lab")
	if err != nil {
		t.Fatal(err)
	}
	p, _ := profile.Parse(`{"goos":"linux","goarch":"amd64"}`)
	got, err := Open(context.Background(), root, p)
	if err != nil {
		t.Fatal(err)
	}
	if len(got.Packages) != 3 {
		t.Fatalf("packages=%d", len(got.Packages))
	}
	for _, pkg := range got.Packages {
		t.Log(pkg.ID, len(pkg.Syntax), pkg.Errors)
		if len(pkg.Syntax) == 0 || pkg.IllTyped {
			t.Fatal(pkg.ID, pkg.Errors)
		}
	}
}

func TestOnlyGoListInvocations(t *testing.T) {
	// Interpose a real go wrapper, recording even indirect packages invocations.
	dir := t.TempDir()
	realGo, err := exec.LookPath("go")
	if err != nil {
		t.Fatal(err)
	}
	log := filepath.Join(dir, "calls")
	script := "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '" + log + "'\nexec '" + realGo + "' \"$@\"\n"
	if err := os.WriteFile(filepath.Join(dir, "go"), []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	TestLoadLab(t)
	TestPreModuleGOPATHFallback(t)
	data, err := os.ReadFile(log)
	if err != nil {
		t.Fatal(err)
	}
	for line := range strings.SplitSeq(strings.TrimSpace(string(data)), "\n") {
		arg := strings.Fields(line)[0]
		if arg != "list" && arg != "version" && arg != "env" {
			t.Fatalf("forbidden invocation %q", line)
		}
	}
}

func TestGopathFlagDoesNotOverrideOwnGoMod(t *testing.T) {
	// A root with its own go.mod must stay in module mode even when the flag is
	// set: gopath_import is only the pre-module fallback, never a blanket.
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "go.mod"), []byte("module demo\n\ngo 1.22\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "a.go"), []byte("package demo\nfunc A() int { return 1 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	p, _ := profile.Parse(`{"goos":"linux","goarch":"amd64","gopath_import":"should/be/ignored"}`)
	got, err := Open(context.Background(), root, p)
	if err != nil {
		t.Fatal(err)
	}
	if len(got.Packages) != 1 || got.Packages[0].PkgPath != "demo" {
		t.Fatalf("expected module mode (demo), got %v", got.Packages)
	}
}

func TestGopathEnvLocked(t *testing.T) {
	t.Setenv("GOPROXY", "https://invalid")
	t.Setenv("GOFLAGS", "-mod=mod")
	t.Setenv("GOPACKAGESDRIVER", "malicious")
	p, _ := profile.Parse(`{"goos":"linux","goarch":"amd64","gopath_import":"x/y"}`)
	env := strings.Join(gopathEnv(p, "/tmp/ske-gopath.test"), "\n")
	for _, want := range []string{"GO111MODULE=off", "GOFLAGS=", "GOPATH=/tmp/ske-gopath.test", "GOTOOLCHAIN=local", "GOPROXY=off", "CGO_ENABLED=0", "GOENV=off", "GOEXPERIMENT=", "GOWORK=off"} {
		if !strings.Contains(env, want) {
			t.Fatal(want, env)
		}
	}
	if strings.Contains(env, "malicious") || strings.Contains(env, "https://invalid") {
		t.Fatal(env)
	}
	// Module mode must not silently gain the GOPATH override, and the fallback
	// must clear the module-only -mod flag.
	if mod := strings.Join(Env(p), "\n"); strings.Contains(mod, "GO111MODULE=") {
		t.Fatal("module mode env must not set GO111MODULE", mod)
	}
}

func TestPreModuleGOPATHFallback(t *testing.T) {
	root, err := filepath.Abs("../../testdata/legacy-project")
	if err != nil {
		t.Fatal(err)
	}
	if _, e := os.Stat(filepath.Join(root, "go.mod")); e == nil {
		t.Fatal("fixture must stay module-less")
	}
	p, err := profile.Parse(`{"goos":"linux","goarch":"amd64","gopath_import":"legacy.example.com/legacypkg"}`)
	if err != nil {
		t.Fatal(err)
	}
	if p.GopathImport == "" {
		t.Fatal("profile flag did not parse")
	}
	got, err := Open(context.Background(), root, p)
	if err != nil {
		t.Fatal(err)
	}
	if len(got.Packages) != 1 {
		t.Fatalf("packages=%d", len(got.Packages))
	}
	pkg := got.Packages[0]
	if pkg.PkgPath != "legacy.example.com/legacypkg" {
		t.Fatalf("PkgPath=%q", pkg.PkgPath)
	}
	if want := "go:legacy.example.com/legacypkg@linux/amd64"; got.UnitKey(pkg) != want {
		t.Fatalf("unit key = %q, want %q", got.UnitKey(pkg), want)
	}
	if pkg.Module != nil {
		t.Fatalf("module metadata must be absent in GOPATH mode, got %+v", pkg.Module)
	}
	for _, pkg := range got.Packages {
		if len(pkg.Syntax) == 0 || pkg.IllTyped || len(pkg.Errors) > 0 {
			t.Fatal(pkg.ID, pkg.Errors)
		}
		for _, f := range pkg.GoFiles {
			rel, e := filepath.Rel(root, f)
			if e != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
				t.Fatalf("source path escaped project root: %s", f)
			}
		}
	}
	// Two loads must produce identical fingerprints: the temp GOPATH path is
	// random, so any leaked temp path or module metadata would break equality.
	repeat, err := Open(context.Background(), root, p)
	if err != nil {
		t.Fatal(err)
	}
	if repeat.Fingerprint != got.Fingerprint {
		t.Fatalf("fingerprint not deterministic\nfirst=%s\nsecond=%s", got.Fingerprint, repeat.Fingerprint)
	}
}

func TestPreModuleNoFlagIsExplicitFrontier(t *testing.T) {
	// A genuinely module-less root (no go.mod in the root or any ancestor), like
	// the real go-cache/way/pat/columnize clones: without the gopath_import flag
	// the adapter must fail explicitly instead of loading with a machine-dependent
	// "_/abs/path" import path.
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "a.go"), []byte("package x\nfunc A() int { return 1 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, e := os.Stat(filepath.Join(root, "go.mod")); e == nil {
		t.Fatal("fixture must stay module-less")
	}
	p, _ := profile.Parse(`{"goos":"linux","goarch":"amd64"}`)
	_, err := Open(context.Background(), root, p)
	if err == nil {
		t.Fatal("module-less without the gopath_import flag must fail explicitly")
	}
	if !strings.HasPrefix(err.Error(), "PROJECT_MODULE_REQUIRED") {
		t.Fatalf("wrong error: %v", err)
	}
	if !strings.Contains(err.Error(), "gopath_import") {
		t.Fatalf("error must be actionable: %v", err)
	}
}
