package example

func Sum(n int) int {
	s := 0
	for i := 0; i < n; i++ {
		s += i
	}
	return s
}

func Count3() int {
	c := 0
	for i := 0; i < 3; i++ {
		c++
	}
	return c
}

func SumRange(xs []int) int {
	s := 0
	for _, x := range xs {
		s += x
	}
	return s
}
