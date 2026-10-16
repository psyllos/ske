package example

func AllocTwice() bool {
	var first *int
	for i := 0; i < 2; i++ {
		p := new(int)
		if i == 0 {
			first = p
			*p = 1
		} else {
			*p = 2
		}
	}
	if *first != 1 {
		return false
	}
	return true
}
