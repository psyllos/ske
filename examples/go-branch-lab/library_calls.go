package example

import "strings"

func HasA(s string) bool {
	if strings.HasPrefix(s, "a") {
		return true
	}
	return false
}
