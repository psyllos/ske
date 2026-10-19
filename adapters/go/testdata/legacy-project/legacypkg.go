// Package legacypkg is a hand-written pre-module (GOPATH-era) fixture with no go.mod.
// It exists only to exercise the adapter's pre-module fallback and is not built
// by SKE: loading is source-only and no test/build/generate is ever invoked.
package legacypkg

import (
	"errors"
	"strings"
)

// ErrEmpty is returned by Split on input without words.
var ErrEmpty = errors.New("legacypkg: empty")

// Greet builds a greeting from a name.
func Greet(name string) string {
	return "hello, " + name
}

// WordCount counts the space-separated words in s.
func WordCount(s string) int {
	return len(strings.Fields(s))
}

// Split splits s on spaces and returns the non-empty words, or ErrEmpty.
func Split(s string) ([]string, error) {
	words := strings.Fields(s)
	if len(words) == 0 {
		return nil, ErrEmpty
	}
	return words, nil
}
