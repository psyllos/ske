package example

import "errors"

var ErrUnderage = errors.New("underage")
var ErrDisabled = errors.New("disabled")

func Check(age int, enabled bool) error {
	if age < 18 {
		return ErrUnderage
	}
	if !enabled {
		return ErrDisabled
	}
	return nil
}
