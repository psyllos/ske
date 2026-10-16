package example

func OrDecision(a, b bool) bool {
	if a || b {
		return true
	}
	return false
}

func AndDecision(a, b bool) bool {
	if a && b {
		return true
	}
	return false
}

func MixedDecision(a, b, c bool) bool {
	if a || (b && c) {
		return true
	}
	return false
}
