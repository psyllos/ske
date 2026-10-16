package example

var MutableTotal int

func ReadGlobal() int {
	return MutableTotal
}

func ReadChangingGlobal() (int, int) {
	a := ReadGlobal()
	MutableTotal++
	b := ReadGlobal()
	return a, b
}
