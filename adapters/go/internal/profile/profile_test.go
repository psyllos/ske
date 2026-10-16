package profile

import "testing"

func TestOptions(t *testing.T) {
	p, e := Parse(`{"profile_version":1,"language":"go","goos":"linux","goarch":"386","tags":["z","a","a"],"include_tests":true,"cgo":true,"patterns":["./..."],"go_work":"off"}`)
	if e != nil {
		t.Fatal(e)
	}
	if p.GOARCH != "386" || len(p.Tags) != 2 || !p.IncludeTests || !p.Cgo {
		t.Fatal(p)
	}
	for _, text := range []string{`{"unknown":1}`, `{"patterns":[]}`, `{"goos":"bogus"}`, `{"goarch":"bogus"}`, `{"patterns":["-exec=evil"]}`, `{} {}`} {
		if _, e := Parse(text); e == nil {
			t.Fatal("accepted invalid", text)
		}
	}
}
