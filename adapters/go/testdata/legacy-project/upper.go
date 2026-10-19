package legacypkg

import "unicode"

// UppercaseFirst upper-cases the first rune of s.
func UppercaseFirst(s string) string {
	if s == "" {
		return s
	}
	r := []rune(s)
	r[0] = unicode.ToUpper(r[0])
	return string(r)
}
