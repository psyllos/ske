package partial

type errString string

func (e errString) Error() string { return string(e) }

var errPanic = errString("panic")

func Safe(f func()) (err error) {
	defer func() {
		if r := recover(); r != nil {
			err = errPanic
		}
	}()
	f()
	return nil
}

func Boom() {
	panic("boom")
}
