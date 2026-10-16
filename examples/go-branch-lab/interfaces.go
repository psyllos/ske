package example

type Shape interface{ Area() int }

type Square struct{ side int }

func (s Square) Area() int { return s.side * s.side }

type Rect struct{ w, h int }

func (r Rect) Area() int { return r.w * r.h }

func Total(s Shape) int {
	return s.Area()
}
