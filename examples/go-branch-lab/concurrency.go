package example

func Pipe(n int) int {
	ch := make(chan int)
	done := make(chan struct{})
	go func() {
		ch <- n
		close(done)
	}()
	select {
	case v := <-ch:
		return v
	case <-done:
		return -1
	}
}
