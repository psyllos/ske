package example

var sideEffects int

func touch() bool {
	sideEffects++
	return true
}

func And(a, b bool) bool {
	if a && b {
		return true
	}
	return false
}

func Or(a, b bool) bool {
	if a || b {
		return true
	}
	return false
}

func AndValue(a, b bool) bool {
	return a && b
}

func AndCall(a bool) bool {
	if a && touch() {
		return true
	}
	return false
}
