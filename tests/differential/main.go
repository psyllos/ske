// Trusted acceptance runner. Never execute arbitrary ingested repositories.
package main

import (
	"encoding/json"
	"example"
	"os"
)

type observation struct {
	Function string `json:"function"`
	X        int    `json:"x"`
	Enabled  bool   `json:"enabled"`
	Result   string `json:"result"`
}

func main() {
	var out []observation
	for _, x := range []int{-128, -11, -10, -1, 0, 1, 17, 18, 126, 127} {
		r := example.Decide(x)
		b, _ := json.Marshal(r)
		out = append(out, observation{"Decide", x, false, string(b)})
		for _, enabled := range []bool{false, true} {
			result := "nil"
			if err := example.Check(x, enabled); err != nil {
				result = err.Error()
			}
			out = append(out, observation{"Check", x, enabled, result})
		}
		b, _ = json.Marshal(example.AddOne(int8(x)))
		out = append(out, observation{"AddOne", x, false, string(b)})
	}
	if err := json.NewEncoder(os.Stdout).Encode(out); err != nil {
		panic(err)
	}
}
