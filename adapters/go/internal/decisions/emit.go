// Package decisions inventories source-level control-flow decisions.
package decisions

import (
	"go/ast"
	"go/token"
	"path/filepath"
	"sort"
	"strconv"

	cir "ske/adapters/go/gen/cir/v1"
)

// Function is one emitted function, including each supplied generic instance.
type Function struct {
	ID   string
	Node ast.Node
}

// Decision retains AST anchors for subsequent source-to-IR mapping.
// Condition is nil for keyword-only switch and select decisions.
type Decision struct {
	Record    *cir.SourceDecisionDef
	Node      ast.Node
	Condition ast.Expr
}

// Emit inventories each function independently, in input function order and
// lexical decision order. Nested literals belong to their own inventory entry.
func Emit(fset *token.FileSet, root string, functions []Function) []Decision {
	var result []Decision
	for _, fn := range functions {
		var body *ast.BlockStmt
		switch n := fn.Node.(type) {
		case *ast.FuncDecl:
			body = n.Body
		case *ast.FuncLit:
			body = n.Body
		}
		if body == nil {
			continue
		}
		var entries []Decision
		parents := map[*cir.SourceDecisionDef]*cir.SourceDecisionDef{}
		handled := map[ast.Expr]bool{}
		add := func(kind string, node ast.Node, cond ast.Expr, start, end token.Pos, labels []string) *cir.SourceDecisionDef {
			a, b := fset.PositionFor(start, false), fset.PositionFor(end, false)
			path := a.Filename
			if rel, err := filepath.Rel(root, path); err == nil {
				path = rel
			}
			r := &cir.SourceDecisionDef{FunctionId: fn.ID, DecisionKind: kind,
				Span: &cir.Span{Path: filepath.ToSlash(path), StartLine: uint32(a.Line), StartColumn: uint32(a.Column), EndLine: uint32(b.Line), EndColumn: uint32(b.Column)}}
			for _, label := range labels {
				r.Outcomes = append(r.Outcomes, &cir.SourceOutcomeDef{Label: label})
			}
			entries = append(entries, Decision{Record: r, Node: node, Condition: cond})
			return r
		}
		bools := []string{"TRUE", "FALSE"}
		var atoms func(ast.Expr) []ast.Expr
		atoms = func(e ast.Expr) []ast.Expr {
			handled[e] = true
			switch n := e.(type) {
			case *ast.ParenExpr:
				if parts := atoms(n.X); len(parts) > 1 {
					return parts
				}
			case *ast.UnaryExpr:
				if n.Op == token.NOT {
					if parts := atoms(n.X); len(parts) > 1 {
						return parts
					}
				}
			case *ast.BinaryExpr:
				if n.Op == token.LAND || n.Op == token.LOR {
					handled[e] = true
					return append(atoms(n.X), atoms(n.Y)...)
				}
			}
			return []ast.Expr{e}
		}
		logical := func(e ast.Expr, parent *cir.SourceDecisionDef) {
			parts := atoms(e)
			if len(parts) < 2 {
				return
			}
			if parent == nil {
				parts = parts[:len(parts)-1]
			}
			for _, part := range parts {
				r := add("LOGICAL_OPERAND", part, part, part.Pos(), part.End(), bools)
				if parent != nil {
					parents[r] = parent
				}
			}
		}
		clauses := func(body *ast.BlockStmt, selectStmt bool) []string {
			var labels []string
			hasDefault := false
			for _, stmt := range body.List {
				isDefault := false
				switch c := stmt.(type) {
				case *ast.CaseClause:
					isDefault = c.List == nil
				case *ast.CommClause:
					isDefault = c.Comm == nil
				}
				if isDefault {
					hasDefault = true
				} else {
					labels = append(labels, "CASE:"+strconv.Itoa(len(labels)))
				}
			}
			if !selectStmt || hasDefault {
				labels = append(labels, "DEFAULT")
			}
			return labels
		}
		ast.Inspect(body, func(n ast.Node) bool {
			if n == nil {
				return false
			}
			if _, ok := n.(*ast.FuncLit); ok {
				return false
			}
			switch s := n.(type) {
			case *ast.IfStmt:
				r := add("IF", s, s.Cond, s.Cond.Pos(), s.Cond.End(), bools)
				logical(s.Cond, r)
			case *ast.ForStmt:
				if s.Cond != nil {
					r := add("LOOP_CONDITION", s, s.Cond, s.Cond.Pos(), s.Cond.End(), []string{"ENTER_BODY", "EXIT"})
					logical(s.Cond, r)
				}
			case *ast.RangeStmt:
				add("RANGE_ITERATION", s, s.X, s.X.Pos(), s.X.End(), []string{"ENTER_BODY", "EXIT"})
			case *ast.SwitchStmt:
				start, end := s.Switch, s.Switch+token.Pos(len("switch"))
				if s.Tag != nil {
					start, end = s.Tag.Pos(), s.Tag.End()
				}
				add("SWITCH", s, s.Tag, start, end, clauses(s.Body, false))
			case *ast.TypeSwitchStmt:
				var expr ast.Expr
				switch a := s.Assign.(type) {
				case *ast.AssignStmt:
					expr = a.Rhs[0]
				case *ast.ExprStmt:
					expr = a.X
				}
				x := expr.(*ast.TypeAssertExpr).X
				add("TYPE_SWITCH", s, x, x.Pos(), x.End(), clauses(s.Body, false))
			case *ast.SelectStmt:
				add("SELECT", s, nil, s.Select, s.Select+token.Pos(len("select")), clauses(s.Body, true))
			case ast.Expr:
				if !handled[s] {
					logical(s, nil)
				}
			}
			return true
		})
		sort.SliceStable(entries, func(i, j int) bool {
			a, b := entries[i].Record, entries[j].Record
			x, y := a.Span, b.Span
			if x.StartLine != y.StartLine {
				return x.StartLine < y.StartLine
			}
			if x.StartColumn != y.StartColumn {
				return x.StartColumn < y.StartColumn
			}
			if x.EndLine != y.EndLine {
				return x.EndLine > y.EndLine
			}
			if x.EndColumn != y.EndColumn {
				return x.EndColumn > y.EndColumn
			}
			return a.DecisionKind < b.DecisionKind
		})
		for i, d := range entries {
			d.Record.Id = fn.ID + "/sd" + strconv.Itoa(i)
			for j, o := range d.Record.Outcomes {
				o.Id = d.Record.Id + "/o" + strconv.Itoa(j)
			}
		}
		for _, d := range entries {
			if p := parents[d.Record]; p != nil {
				d.Record.ParentDecisionId = p.Id
			}
		}
		result = append(result, entries...)
	}
	return result
}
