package example

import "cmp"

func Max[T cmp.Ordered](a, b T) T {
	if a > b {
		return a
	}
	return b
}

func UseMax() (int, string) {
	return Max(1, 2), Max("a", "b")
}
