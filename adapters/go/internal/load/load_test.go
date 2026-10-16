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
	if err := os.WriteFile(filepath.Join(dir, "go"), []byte(script), 0700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	TestLoadLab(t)
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
