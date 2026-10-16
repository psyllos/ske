package example

func AddOne(x int8) int8 {
	y := x + 1
	if y < x {
		return 1
	}
	return 0
}

func Big(x int) bool {
	return x+1 < x
}
