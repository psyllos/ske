package example

import "errors"

var ErrZero = errors.New("zero")

func DivMod(a, b int) (int, error) {
	if b == 0 {
		return 0, ErrZero
	}
	return a / b, nil
}

func UseDivMod() int {
	q, err := DivMod(4, 2)
	if err != nil {
		return -1
	}
	return q
}
