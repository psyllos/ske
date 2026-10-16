package example

func MemNoCall(p *int) int {
	if *p < 0 {
		return -1
	}
	if *p < -10 {
		return -2
	}
	return 0
}

var Hook func()

func MemOpaque(p *int) int {
	if *p < 0 {
		return -1
	}
	Hook()
	if *p < -10 {
		return -2
	}
	return 0
}

func MemAlias(p, q *int) int {
	*p = 1
	if *q == 1 {
		return 1
	}
	return 0
}

func MemAlloc(p *int) int {
	*p = 1
	a := new(int)
	*a = 2
	if *p == 1 {
		return 1
	}
	return 0
}
