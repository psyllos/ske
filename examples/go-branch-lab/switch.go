package example

func Day(n int) string {
	switch n {
	case 1:
		return "one"
	case 2, 3:
		return "few"
	default:
		return "many"
	}
}

func Sign(n int) int {
	switch {
	case n < 0:
		return -1
	case n > 0:
		return 1
	}
	return 0
}
