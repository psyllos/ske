package example

func Decide(x int) int {
	if x < 0 {
		return -1
	}
	if x < -10 {
		return -2
	}
	return 0
}
