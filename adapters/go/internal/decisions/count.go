package decisions

import (
	"go/ast"
	"go/token"
)

// Count is an independent AST traversal. It deliberately shares no syntax
// classification or traversal helpers with Emit.
func Count(functions []Function) uint64 {
	var total uint64
	for _, fn := range functions {
		var body *ast.BlockStmt
		switch f := fn.Node.(type) {
		case *ast.FuncDecl:
			body = f.Body
		case *ast.FuncLit:
			body = f.Body
		}
		if body == nil {
			continue
		}
		covered := make(map[ast.Expr]bool)
		var leaves func(ast.Expr) uint64
		leaves = func(e ast.Expr) uint64 {
			covered[e] = true
			switch x := e.(type) {
			case *ast.ParenExpr:
				return leaves(x.X)
			case *ast.UnaryExpr:
				if x.Op == token.NOT {
					return leaves(x.X)
				}
			case *ast.BinaryExpr:
				if x.Op == token.LAND || x.Op == token.LOR {
					covered[e] = true
					return leaves(x.X) + leaves(x.Y)
				}
			}
			return 1
		}
		ast.Inspect(body, func(node ast.Node) bool {
			if node == nil {
				return false
			}
			if _, literal := node.(*ast.FuncLit); literal {
				return false
			}
			var condition ast.Expr
			switch x := node.(type) {
			case *ast.IfStmt:
				total++
				condition = x.Cond
			case *ast.ForStmt:
				if x.Cond != nil {
					total++
					condition = x.Cond
				}
			case *ast.SwitchStmt, *ast.TypeSwitchStmt, *ast.RangeStmt, *ast.SelectStmt:
				total++
			}
			if condition != nil {
				n := leaves(condition)
				if n > 1 {
					total += n
				}
			}
			if e, ok := node.(ast.Expr); ok && !covered[e] {
				n := leaves(e)
				if n > 1 {
					total += n - 1
				}
			}
			return true
		})
	}
	return total
}
