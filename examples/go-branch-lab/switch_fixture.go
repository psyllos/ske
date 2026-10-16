package example

func DaySwitch(n int) int {
	switch n {
	case 1:
		return 10
	case 2, 3:
		return 20
	default:
		return 30
	}
}
