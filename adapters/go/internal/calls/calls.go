// Package calls resolves SSA calls without asserting a closed world for CHA.
package calls

import (
	"go/types"
	"sort"

	"golang.org/x/tools/go/callgraph/cha"
	"golang.org/x/tools/go/ssa"
)

type Resolution struct {
	Dispatch string
	Kind     string
	Closed   bool
	Targets  []*ssa.Function
}

type Resolver struct {
	targets map[ssa.CallInstruction][]*ssa.Function
}

func New(program *ssa.Program) *Resolver {
	r := &Resolver{targets: make(map[ssa.CallInstruction][]*ssa.Function)}
	graph := cha.CallGraph(program)
	for _, node := range graph.Nodes {
		for _, edge := range node.Out {
			if edge.Site != nil {
				r.targets[edge.Site] = append(r.targets[edge.Site], edge.Callee.Func)
			}
		}
	}
	return r
}

func (r *Resolver) Resolve(site ssa.CallInstruction) Resolution {
	c := site.Common()
	if fn := c.StaticCallee(); fn != nil {
		return Resolution{"STATIC", "STATIC", true, []*ssa.Function{fn}}
	}
	dispatch := "DYNAMIC"
	if c.IsInvoke() {
		dispatch = "INTERFACE"
	}
	out := Resolution{Dispatch: dispatch, Kind: "CHA"}
	seen := map[*ssa.Function]bool{}
	for _, fn := range r.targets[site] {
		// Retain only signature-compatible function values. In particular,
		// package initialization and unrelated dependency signatures are not
		// candidates for an arbitrary dynamic call.
		if !c.IsInvoke() && !types.Identical(c.Signature(), fn.Signature) {
			continue
		}
		if !seen[fn] {
			seen[fn] = true
			out.Targets = append(out.Targets, fn)
		}
	}
	sort.Slice(out.Targets, func(i, j int) bool { return out.Targets[i].String() < out.Targets[j].String() })
	return out
}
