// Package profile parses the closed build-profile contract.
package profile

import (
	"encoding/json"
	"fmt"
	"io"
	"runtime"
	"slices"
	"strings"
)

type Profile struct {
	Version      int      `json:"profile_version"`
	Language     string   `json:"language"`
	GoVersion    *string  `json:"go_version"`
	GOOS         string   `json:"goos"`
	GOARCH       string   `json:"goarch"`
	Tags         []string `json:"tags"`
	IncludeTests bool     `json:"include_tests"`
	Cgo          bool     `json:"cgo"`
	Patterns     []string `json:"patterns"`
	GoWork       *string  `json:"go_work"`
}

func Parse(text string) (Profile, error) {
	p := Profile{Version: 1, Language: "go", GOOS: runtime.GOOS, GOARCH: runtime.GOARCH, Tags: []string{}, Patterns: []string{"./..."}}
	if strings.TrimSpace(text) != "" {
		d := json.NewDecoder(strings.NewReader(text))
		d.DisallowUnknownFields()
		if err := d.Decode(&p); err != nil {
			return p, err
		}
		var trailing any
		if err := d.Decode(&trailing); err != io.EOF {
			return p, fmt.Errorf("trailing profile content")
		}
	}
	if p.Version != 1 || p.Language != "go" || len(p.Patterns) == 0 {
		return p, fmt.Errorf("invalid profile version, language or patterns")
	}
	// Supported pairs are maintained by the executing toolchain's go/build
	// release table, not by spawning `go tool` (forbidden in analyzed workflows).
	valid := map[string][]string{
		"aix": {"ppc64"}, "android": {"386", "amd64", "arm", "arm64"},
		"darwin": {"amd64", "arm64"}, "dragonfly": {"amd64"},
		"freebsd": {"386", "amd64", "arm", "arm64", "riscv64"}, "illumos": {"amd64"},
		"ios": {"amd64", "arm64"}, "js": {"wasm"},
		"linux":  {"386", "amd64", "arm", "arm64", "loong64", "mips", "mipsle", "mips64", "mips64le", "ppc64", "ppc64le", "riscv64", "s390x"},
		"netbsd": {"386", "amd64", "arm", "arm64"}, "openbsd": {"386", "amd64", "arm", "arm64", "ppc64", "riscv64"},
		"plan9": {"386", "amd64", "arm"}, "solaris": {"amd64"}, "wasip1": {"wasm"}, "windows": {"386", "amd64", "arm64"},
	}
	if !slices.Contains(valid[p.GOOS], p.GOARCH) {
		return p, fmt.Errorf("unsupported target %s/%s", p.GOOS, p.GOARCH)
	}
	for _, s := range append(slices.Clone(p.Patterns), p.Tags...) {
		if s == "" || strings.HasPrefix(s, "-") || strings.ContainsAny(s, "\x00\r\n") {
			return p, fmt.Errorf("invalid pattern or tag")
		}
	}
	slices.Sort(p.Tags)
	p.Tags = slices.Compact(p.Tags)
	return p, nil
}
