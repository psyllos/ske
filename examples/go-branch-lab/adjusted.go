package example

func Adjusted(x int) int {
	y := 0
	if x > 10 {
		y = 3
	}
	return x - y
}
