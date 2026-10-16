// Package link maps AST evaluation paths to actual SSA branch outcomes.
package link

import (
	"go/ast"
	"go/constant"
	"go/token"
	"slices"
	"strings"

	"golang.org/x/tools/go/ssa"
	cir "ske/adapters/go/gen/cir/v1"
	"ske/adapters/go/internal/decisions"
	"ske/adapters/go/internal/lower"
)

type binding struct {
	branch   lower.Branch
	exact    bool
	inverted bool
}
type term struct {
	expr  ast.Expr
	value bool
	// Negations enclosing a compound expression, before the terminal atom.
	negated bool
}
type path []term

const MaxSourceActivationCases = 4096

// Enumerate lazily: even intermediate Cartesian products must stay bounded.
func paths(e ast.Expr, value bool) []path {
	out, _ := boundedPaths(e, value, MaxSourceActivationCases)
	return out
}

func boundedPaths(e ast.Expr, value bool, limit int) ([]path, bool) {
	var out []path
	complete := walkEvaluation(e, value, false, nil, func(p path) bool {
		out = append(out, slices.Clone(p))
		return len(out) < limit
	})
	return out, complete
}

func walkEvaluation(e ast.Expr, value, negated bool, prefix path, emit func(path) bool) bool {
	switch n := e.(type) {
	case *ast.ParenExpr:
		return walkEvaluation(n.X, value, negated, prefix, emit)
	case *ast.UnaryExpr:
		if n.Op == token.NOT && compound(n.X) {
			return walkEvaluation(n.X, !value, !negated, prefix, emit)
		}
	case *ast.BinaryExpr:
		both := func(left, right bool) bool {
			return walkEvaluation(n.X, left, negated, prefix, func(p path) bool {
				return walkEvaluation(n.Y, right, negated, p, emit)
			})
		}
		if n.Op == token.LAND {
			if value {
				return both(true, true)
			}
			return walkEvaluation(n.X, false, negated, prefix, emit) && both(true, false)
		}
		if n.Op == token.LOR {
			if !value {
				return both(false, false)
			}
			return walkEvaluation(n.X, true, negated, prefix, emit) && both(false, true)
		}
	}
	return emit(append(prefix, term{expr: e, value: value, negated: negated}))
}

// Binding needs atoms, not exponentially many evaluation paths.
func atoms(e ast.Expr) []ast.Expr {
	switch n := e.(type) {
	case *ast.ParenExpr:
		return atoms(n.X)
	case *ast.UnaryExpr:
		if n.Op == token.NOT && compound(n.X) {
			return atoms(n.X)
		}
	case *ast.BinaryExpr:
		if n.Op == token.LAND || n.Op == token.LOR {
			return append(atoms(n.X), atoms(n.Y)...)
		}
	}
	return []ast.Expr{e}
}
func compound(e ast.Expr) bool {
	switch n := e.(type) {
	case *ast.ParenExpr:
		return compound(n.X)
	case *ast.UnaryExpr:
		return n.Op == token.NOT && compound(n.X)
	case *ast.BinaryExpr:
		return n.Op == token.LAND || n.Op == token.LOR
	}
	return false
}
func strip(e ast.Expr) (ast.Expr, bool) {
	invert := false
	for {
		switch n := e.(type) {
		case *ast.ParenExpr:
			e = n.X
		case *ast.UnaryExpr:
			if n.Op != token.NOT {
				return e, invert
			}
			invert = !invert
			e = n.X
		default:
			return e, invert
		}
	}
}
func contains(n ast.Node, p token.Pos) bool {
	return n != nil && p.IsValid() && n.Pos() <= p && p < n.End()
}

// Build mutates only activation attestations in supplied decisions. Missing
// mappings always remain explicit diagnostics, never invented exact links.
func Build(ds []decisions.Decision, branches []lower.Branch) []*cir.Record {
	return BuildWithLimit(ds, branches, MaxSourceActivationCases)
}

// BuildWithLimit bounds cases per source outcome, preserving the full inventory.
// Reaching the limit (including exactly the limit) cannot attest completeness.
func BuildWithLimit(ds []decisions.Decision, branches []lower.Branch, limit int) []*cir.Record {
	limit = max(1, limit)
	var records []*cir.Record
	bindings := map[ast.Expr]binding{}
	used := map[*ssa.If]bool{}
	// DebugRef anchors take precedence over instruction positions.
	for _, b := range branches {
		var anchor ast.Expr
		pos := token.NoPos
		exact := false
		for _, ins := range b.If.Block().Instrs {
			if ins == b.If {
				break
			}
			if d, ok := ins.(*ssa.DebugRef); ok && d.X == b.If.Cond {
				anchor = d.Expr
				exact = true
			}
		}
		if anchor != nil {
			pos = anchor.Pos()
		} else if ins, ok := b.If.Cond.(ssa.Instruction); ok && contains(b.Function.Syntax(), ins.Pos()) {
			pos = ins.Pos()
			exact = true
		}
		var chosen ast.Expr
		for _, d := range ds {
			if d.Record.FunctionId != functionID(b, ds) || d.Condition == nil {
				continue
			}
			if d.Record.DecisionKind != "IF" && d.Record.DecisionKind != "LOOP_CONDITION" && d.Record.DecisionKind != "LOGICAL_OPERAND" {
				continue
			}
			for _, atom := range atoms(d.Condition) {
				base, _ := strip(atom)
				if contains(base, pos) && (chosen == nil || atom.End()-atom.Pos() < chosen.End()-chosen.Pos()) {
					chosen = atom
				}
			}
		}
		if chosen != nil {
			_, inv := strip(chosen)
			bindings[chosen] = binding{b, exact, inv}
			used[b.If] = true
		}
	}
	// Normative fallback pairs unanchored conditions/atoms and branches in
	// lexical decision / block order. It cannot certify activation completeness.
	for _, d := range ds {
		if d.Condition == nil || compound(d.Condition) {
			continue
		}
		kind := d.Record.DecisionKind
		if kind != "IF" && kind != "LOOP_CONDITION" && kind != "LOGICAL_OPERAND" {
			continue
		}
		if _, ok := lookup(bindings, d.Condition); ok {
			continue
		}
		for _, b := range branches {
			if used[b.If] || functionID(b, ds) != d.Record.FunctionId {
				continue
			}
			_, inv := strip(d.Condition)
			bindings[d.Condition] = binding{b, false, inv}
			used[b.If] = true
			break
		}
	}
	for _, d := range ds {
		r := d.Record
		if r.DecisionKind == "SWITCH" || r.DecisionKind == "TYPE_SWITCH" {
			records = append(records, mapSwitch(d, branches, limit)...)
		} else if r.DecisionKind == "SELECT" || r.DecisionKind == "RANGE_ITERATION" {
			records = append(records, mapStructural(d, branches)...)
		} else {
			for oi, o := range r.Outcomes {
				seenLinks := map[string]bool{}
				alternatives, complete := boundedPaths(d.Condition, oi == 0, limit)
				ordinal := 0
				for _, p := range alternatives {
					var edges []string
					quality := "EXACT"
					valid := true
					for _, t := range p {
						b, ok := lookup(bindings, t.expr)
						if !ok {
							valid = false
							complete = false
							break
						}
						id := b.branch.TrueEdgeID
						if t.value == b.inverted {
							id = b.branch.FalseEdgeID
						}
						edges = append(edges, id)
						if !b.exact {
							quality = "APPROX"
							complete = false
						}
					}
					if !valid {
						continue
					}
					records = append(records, activation(o.Id, ordinal, edges, quality))
					ordinal++
					// Compound parent provenance points to the terminal edge.
					if len(edges) > 0 {
						id := edges[len(edges)-1]
						if !seenLinks[id] {
							kind := "DIRECT"
							if compound(d.Condition) || r.DecisionKind == "LOGICAL_OPERAND" {
								kind = "SHORT_CIRCUIT"
							}
							if r.DecisionKind == "LOOP_CONDITION" {
								kind = "LOOP_CONDITION"
							}
							_, inv := strip(p[len(p)-1].expr)
							inv = inv != p[len(p)-1].negated
							records = append(records, provenance(o.Id, id, kind, quality, inv))
							seenLinks[id] = true
						}
					}
				}
				o.ActivationMappingComplete = complete && len(alternatives) > 0
			}
		}
		for _, o := range r.Outcomes {
			if limit == 1 {
				o.ActivationMappingComplete = false
			}
			if !o.ActivationMappingComplete {
				records = append(records, diagnostic(r, "SOURCE_ACTIVATION_INCOMPLETE", "Source outcome "+o.Label+" lacks a complete exact activation mapping"))
			}
		}
	}
	return records
}

func lookup(m map[ast.Expr]binding, e ast.Expr) (binding, bool) {
	if b, ok := m[e]; ok {
		return b, true
	}
	base, inv := strip(e)
	for k, b := range m {
		other, _ := strip(k)
		if base == other {
			b.inverted = inv
			return b, true
		}
	}
	return binding{}, false
}
func functionID(b lower.Branch, ds []decisions.Decision) string {
	// IDs supplied by the lowering layer disambiguate generic instances that
	// deliberately share the same source syntax.
	if i := strings.LastIndex(b.TrueEdgeID, "/b"); i >= 0 {
		return b.TrueEdgeID[:i]
	}
	return ""
}
func provenance(outcome, edge, kind, quality string, inv bool) *cir.Record {
	return &cir.Record{Content: &cir.Record_SourceIrLink{SourceIrLink: &cir.SourceIrLinkDef{SourceOutcomeId: outcome, ControlEdgeId: edge, MappingKind: kind, Quality: quality, Inverted: inv}}}
}
func activation(outcome string, ordinal int, edges []string, quality string) *cir.Record {
	return &cir.Record{Content: &cir.Record_SourceOutcomeActivationCase{SourceOutcomeActivationCase: &cir.SourceOutcomeActivationCaseDef{SourceOutcomeId: outcome, CaseOrdinal: uint32(ordinal), OrderedControlEdgeIds: edges, Quality: quality}}}
}
func diagnostic(d *cir.SourceDecisionDef, code, msg string) *cir.Record {
	return &cir.Record{Content: &cir.Record_Diagnostic{Diagnostic: &cir.Diagnostic{Severity: "INFO", Code: code, Message: msg, Span: d.Span, AffectedFunctionId: d.FunctionId, Impact: "SYMBOLIC"}}}
}

func mapSwitch(d decisions.Decision, bs []lower.Branch, limit int) []*cir.Record {
	var clauses []*ast.CaseClause
	switch n := d.Node.(type) {
	case *ast.SwitchStmt:
		for _, s := range n.Body.List {
			clauses = append(clauses, s.(*ast.CaseClause))
		}
	case *ast.TypeSwitchStmt:
		for _, s := range n.Body.List {
			clauses = append(clauses, s.(*ast.CaseClause))
		}
	}
	var records []*cir.Record
	var previous []string
	ordinal := 0
	allExact := true
	for _, c := range clauses {
		if c.List == nil {
			continue
		}
		o := d.Record.Outcomes[ordinal]
		ordinal++
		var matches []lower.Branch
		for _, b := range bs {
			if functionID(b, []decisions.Decision{d}) != d.Record.FunctionId {
				continue
			}
			ins, ok := b.If.Cond.(ssa.Instruction)
			if !ok {
				continue
			}
			for _, expr := range c.List {
				if contains(expr, ins.Pos()) {
					matches = append(matches, b)
					break
				}
			}
		}
		if len(matches) != len(c.List) || len(matches) >= limit {
			allExact = false
		}
		for i, b := range matches {
			edges := append(slices.Clone(previous), b.TrueEdgeID)
			if i < limit {
				records = append(records, activation(o.Id, i, edges, "EXACT"))
			}
			records = append(records, provenance(o.Id, b.TrueEdgeID, "SWITCH_CASE", "EXACT", false))
			previous = append(previous, b.FalseEdgeID)
		}
		o.ActivationMappingComplete = len(matches) == len(c.List) && allExact
	}
	o := d.Record.Outcomes[len(d.Record.Outcomes)-1]
	if allExact && len(previous) > 0 {
		records = append(records, activation(o.Id, 0, previous, "EXACT"), provenance(o.Id, previous[len(previous)-1], "SWITCH_CASE", "EXACT", false))
		o.ActivationMappingComplete = true
	}
	return records
}

func mapStructural(d decisions.Decision, bs []lower.Branch) []*cir.Record {
	if d.Record.DecisionKind == "SELECT" {
		return mapSelect(d, bs)
	}
	// A range header has no source position in go/ssa. Recognize its
	// dataflow, not arbitrary positioned branches inside the loop body.
	var source ssa.Value
	for _, b := range bs {
		if functionID(b, nil) != d.Record.FunctionId {
			continue
		}
		for _, block := range b.Function.Blocks {
			for _, i := range block.Instrs {
				if ref, ok := i.(*ssa.DebugRef); ok && ref.Expr == d.Condition {
					source = ref.X
				}
			}
		}
		break
	}
	var matches []lower.Branch
	for _, b := range bs {
		if functionID(b, []decisions.Decision{d}) != d.Record.FunctionId {
			continue
		}
		if comparison, ok := b.If.Cond.(*ssa.BinOp); ok && comparison.Op == token.LSS {
			length, ok := comparison.Y.(*ssa.Call)
			if !ok || len(length.Call.Args) != 1 || length.Call.Args[0] != source || source == nil {
				continue
			}
			builtin, ok := length.Call.Value.(*ssa.Builtin)
			if !ok || builtin.Name() != "len" {
				continue
			}
			increment, ok := comparison.X.(*ssa.BinOp)
			if !ok || increment.Op != token.ADD {
				continue
			}
			phi, ok := increment.X.(*ssa.Phi)
			step, constantStep := increment.Y.(*ssa.Const)
			if ok && phi.Block() == b.If.Block() && constantStep && step.Value != nil && constant.Compare(step.Value, token.EQL, constant.MakeInt64(1)) {
				matches = append(matches, b)
			}
		} else if extract, ok := b.If.Cond.(*ssa.Extract); ok {
			if extract.Index == 0 {
				if next, ok := extract.Tuple.(*ssa.Next); ok {
					if iterator, ok := next.Iter.(*ssa.Range); ok && source != nil && iterator.X == source {
						matches = append(matches, b)
					}
				}
			} else if extract.Index == 1 {
				// Channel range tests the comma-ok receive generated at the
				// range header, not another receive on that channel in its body.
				if recv, ok := extract.Tuple.(*ssa.UnOp); ok && recv.Op == token.ARROW &&
					recv.CommaOk && recv.X == source && source != nil && recv.Pos() == d.Node.Pos() {
					matches = append(matches, b)
				}
			}
		}
	}
	if d.Record.DecisionKind == "RANGE_ITERATION" && len(matches) == 1 {
		b := matches[0]
		var out []*cir.Record
		for i, o := range d.Record.Outcomes {
			edge := b.TrueEdgeID
			if i == 1 {
				edge = b.FalseEdgeID
			}
			o.ActivationMappingComplete = true
			out = append(out, activation(o.Id, 0, []string{edge}, "EXACT"), provenance(o.Id, edge, "LOOP_CONDITION", "EXACT", false))
		}
		return out
	}
	return nil
}

func mapSelect(d decisions.Decision, bs []lower.Branch) []*cir.Record {
	// Index comparisons must all refer to the same Select in this AST node.
	// Constants identify source communication ordinals; the false chain
	// supplies the activation prerequisites and optional default.
	byIndex := map[int]lower.Branch{}
	var selection *ssa.Select
	for _, b := range bs {
		if functionID(b, nil) != d.Record.FunctionId {
			continue
		}
		comparison, ok := b.If.Cond.(*ssa.BinOp)
		if !ok || comparison.Op != token.EQL {
			continue
		}
		index, ok := comparison.X.(*ssa.Extract)
		if !ok || index.Index != 0 {
			continue
		}
		selectOp, ok := index.Tuple.(*ssa.Select)
		if !ok || selectOp.Pos() != d.Node.Pos() {
			continue
		}
		k, ok := comparison.Y.(*ssa.Const)
		if !ok || k.Value == nil {
			continue
		}
		n, ok := constant.Int64Val(k.Value)
		if !ok || n < 0 || n >= int64(len(selectOp.States)) {
			return nil
		}
		if selection != nil && selection != selectOp {
			return nil
		}
		if _, duplicate := byIndex[int(n)]; duplicate {
			return nil
		}
		selection = selectOp
		byIndex[int(n)] = b
	}
	if selection == nil || len(byIndex) != len(selection.States) {
		return nil
	}
	// Verify the comparison chain before certifying any outcome.
	for i := 1; i < len(byIndex); i++ {
		if byIndex[i-1].If.Block().Succs[1] != byIndex[i].If.Block() {
			return nil
		}
	}
	var out []*cir.Record
	var previous []string
	for i, outcome := range d.Record.Outcomes {
		edges := slices.Clone(previous)
		edge := ""
		if outcome.Label == "DEFAULT" {
			if selection.Blocking || len(previous) == 0 {
				return nil
			}
			edge = previous[len(previous)-1]
		} else {
			b, ok := byIndex[i]
			if !ok {
				return nil
			}
			edge = b.TrueEdgeID
			edges = append(edges, edge)
			previous = append(previous, b.FalseEdgeID)
		}
		outcome.ActivationMappingComplete = true
		out = append(out, activation(outcome.Id, 0, edges, "EXACT"), provenance(outcome.Id, edge, "SWITCH_CASE", "EXACT", false))
	}
	return out
}
