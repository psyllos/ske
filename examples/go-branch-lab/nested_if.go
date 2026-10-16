package example

func Classify(a, b int) int {
	if a > 0 {
		if b > 0 {
			return 1
		}
		return 2
	}
	return 3
}
