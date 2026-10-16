package example

func Entry(a int) int {
	return helper(a)
}

func helper(x int) int {
	if x > 100 {
		return 1
	}
	return 0
}

type Doer interface{ Do(n int) int }

func CallDoer(d Doer, n int) int {
	if d.Do(n) > 0 {
		return 1
	}
	return 0
}

func Fact(n int) int {
	if n <= 1 {
		return 1
	}
	return n * Fact(n-1)
}

func Divide2(a, b int) (int, error) {
	q, err := DivMod(a, b)
	if err != nil {
		return 0, err
	}
	return q, nil
}
