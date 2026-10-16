package lower

import (
	"fmt"
	"go/token"
	"go/types"
	"reflect"

	"golang.org/x/tools/go/ssa"
	cir "ske/adapters/go/gen/cir/v1"
)

func (l *lowerer) exit(i ssa.Instruction, kind string) {
	span := l.span(i.Pos())
	if span == nil {
		// Synthetic receiver wrappers still originate at the declared
		// method; retain that provenance rather than a fabricated token.
		if object := l.fn.Object(); object != nil {
			span = l.span(object.Pos())
		}
	}
	l.exits = append(l.exits, &cir.ImplicitExitSiteDef{FunctionId: l.FunctionIDs[l.fn], Span: span, ExitKind: kind, ModelingStatus: "UNMODELED"})
}

// Allocation/global addresses cannot be nil. Derived field addresses preserve
// this fact; unknown pointers remain conservatively inventoried.
func definitelyNonNil(v ssa.Value) bool {
	switch v := v.(type) {
	case *ssa.Global, *ssa.Alloc:
		return true
	case *ssa.IndexAddr:
		// Producing an element address checks bounds (and, for *array,
		// the base pointer). Inventory those checks on IndexAddr itself.
		return true
	case *ssa.FieldAddr:
		return definitelyNonNil(v.X)
	case *ssa.ChangeType:
		return definitelyNonNil(v.X)
	}
	return false
}

func (l *lowerer) nilExit(i ssa.Instruction, address ssa.Value) {
	if !definitelyNonNil(address) {
		l.exit(i, "NIL_DEREFERENCE")
	}
}
func (l *lowerer) opaque(d *cir.InstructionDef, i ssa.Instruction, construct string) {
	d.Opcode = "OPAQUE_OP"
	d.Payload = &cir.OperationPayload{Details: &cir.OperationPayload_Opaque{Opaque: &cir.OpaqueSemantics{SourceConstruct: construct, Reason: "not precisely modeled by Go adapter"}}}
	l.diag("OPAQUE_OP_EMITTED", "SYMBOLIC", construct, i.Pos())
}
func (l *lowerer) instruction(i ssa.Instruction, bid string, n int) int {
	d := &cir.InstructionDef{Id: fmt.Sprintf("%s/i%d", bid, n), BlockId: bid, Ordinal: uint32(n)}
	if p := l.span(i.Pos()); p != nil {
		d.Spans = []*cir.Span{p}
	}
	if v, ok := i.(ssa.Value); ok {
		d.ResultValueId = l.values[v]
	}
	for _, o := range i.Operands(nil) {
		if o != nil && *o != nil {
			if _, ok := (*o).(*ssa.Builtin); !ok {
				d.OperandValueIds = append(d.OperandValueIds, l.value(*o))
			}
		}
	}
	count := 1
	switch x := i.(type) {
	case *ssa.BinOp:
		ops := map[token.Token]string{token.ADD: "ADD", token.SUB: "SUB", token.MUL: "MUL", token.QUO: "DIV", token.REM: "REM", token.AND: "BIT_AND", token.OR: "BIT_OR", token.XOR: "XOR", token.SHL: "SHL", token.SHR: "SHR", token.EQL: "EQ", token.NEQ: "NE", token.LSS: "LT", token.LEQ: "LE", token.GTR: "GT", token.GEQ: "GE"}
		d.Opcode = ops[x.Op]
		t := l.types[l.typ(x.X.Type())]
		if x.Op == token.AND_NOT {
			tmp := fmt.Sprintf("%s/v%d", l.FunctionIDs[l.fn], l.next)
			l.next++
			l.Records = append(l.Records, &cir.Record{Content: &cir.Record_ValueDef{ValueDef: &cir.ValueDef{Id: tmp, FunctionId: l.FunctionIDs[l.fn], TypeId: l.typ(x.Y.Type()), Role: "SSA"}}})
			not := &cir.InstructionDef{Id: d.Id, BlockId: bid, Ordinal: uint32(n), Opcode: "BIT_NOT", ResultValueId: tmp, OperandValueIds: []string{l.value(x.Y)}, Spans: d.Spans}
			l.Records = append(l.Records, &cir.Record{Content: &cir.Record_InstructionDef{InstructionDef: not}})
			count = 2
			d.Id = fmt.Sprintf("%s/i%d", bid, n+1)
			d.Ordinal++
			d.Opcode = "BIT_AND"
			d.OperandValueIds = []string{l.value(x.X), tmp}
		}
		if t.Kind != "BITVECTOR" && !(t.Kind == "BOOL" && (x.Op == token.EQL || x.Op == token.NEQ)) && !(t.Kind == "STRING" && (x.Op == token.EQL || x.Op == token.NEQ)) && !(x.Op == token.EQL || x.Op == token.NEQ) && t.Kind != "IEEE_FLOAT" {
			l.opaque(d, i, x.Op.String())
		}
		if t.Kind == "IEEE_FLOAT" || (t.Kind == "OPAQUE") || (t.Kind == "STRING" && x.Op != token.EQL && x.Op != token.NEQ) {
			l.opaque(d, i, x.Op.String())
		}
		switch d.Opcode {
		case "ADD", "SUB", "MUL", "DIV", "REM", "SHL", "SHR":
			signed := t.IsSigned
			if x.Op == token.SHL || x.Op == token.SHR {
				signed = l.types[l.typ(x.Y.Type())].IsSigned
			}
			s := "UNSIGNED"
			if signed {
				s = "SIGNED"
			}
			d.Payload = &cir.OperationPayload{Details: &cir.OperationPayload_Binary{Binary: &cir.BinarySemantics{Operator: d.Opcode, OverflowMode: "WRAP", Signedness: s}}}
		}
		if x.Op == token.QUO || x.Op == token.REM {
			l.exit(i, "DIVISION_BY_ZERO")
		}
		if (x.Op == token.SHL || x.Op == token.SHR) && l.types[l.typ(x.Y.Type())].IsSigned {
			l.exit(i, "NEGATIVE_SHIFT")
		}
	case *ssa.UnOp:
		d.Opcode = map[token.Token]string{token.SUB: "NEG", token.NOT: "LOGICAL_NOT", token.XOR: "BIT_NOT", token.MUL: "LOAD", token.ARROW: "CHANNEL_RECV"}[x.Op]
		if d.Opcode == "NEG" {
			d.Payload = &cir.OperationPayload{Details: &cir.OperationPayload_Binary{Binary: &cir.BinarySemantics{Operator: "NEG", OverflowMode: "WRAP", Signedness: "SIGNED"}}}
			if l.types[l.typ(x.X.Type())].Kind != "BITVECTOR" {
				l.opaque(d, i, "unary -")
			}
		}
		if d.Opcode == "LOAD" {
			d.Payload = memory("LOAD")
			l.nilExit(i, x.X)
		}
		if d.Opcode == "CHANNEL_RECV" {
			l.diag("CONCURRENCY_PARTIAL", "CONCURRENCY", "channel receive", i.Pos())
		}
	case *ssa.Phi:
		d.Opcode = "PHI"
		d.OperandValueIds = nil
		d.Payload = &cir.OperationPayload{}
		for k, e := range x.Edges {
			d.Payload.PhiInputs = append(d.Payload.PhiInputs, &cir.PhiInput{PredecessorBlockId: fmt.Sprintf("%s/b%d", l.FunctionIDs[l.fn], x.Block().Preds[k].Index), ValueId: l.value(e)})
		}
	case *ssa.If:
		d.Opcode = "BRANCH"
		d.IsTerminator = true
	case *ssa.Jump:
		d.Opcode = "JUMP"
		d.IsTerminator = true
	case *ssa.Return:
		d.Opcode = "RETURN"
		d.IsTerminator = true
	case *ssa.Panic:
		d.Opcode = "PANIC"
		d.IsTerminator = true
	case *ssa.Alloc:
		d.Opcode = "ALLOC"
		d.Payload = memory("ALLOC")
	case *ssa.Store:
		d.Opcode = "STORE"
		d.Payload = memory("STORE")
		l.nilExit(i, x.Addr)
	case *ssa.FieldAddr:
		d.Opcode = "FIELD_ADDR"
		d.Payload = field(x.Field)
		l.nilExit(i, x.X)
	case *ssa.Field:
		d.Opcode = "EXTRACT_FIELD"
		d.Payload = field(x.Field)
	case *ssa.Extract:
		d.Opcode = "EXTRACT_FIELD"
		d.Payload = field(x.Index)
	case *ssa.Index:
		d.Opcode = "INDEX"
		l.exit(i, "INDEX_OUT_OF_RANGE")
	case *ssa.IndexAddr:
		d.Opcode = "INDEX_ADDR"
		l.exit(i, "INDEX_OUT_OF_RANGE")
		if _, pointer := x.X.Type().Underlying().(*types.Pointer); pointer {
			l.nilExit(i, x.X)
		}
	case *ssa.MakeInterface:
		d.Opcode = "MAKE_INTERFACE"
		d.Payload = typeTest(l.typ(x.X.Type()), false)
	case *ssa.TypeAssert:
		d.Opcode = "DOWNCAST"
		if x.CommaOk {
			d.Opcode = "TYPE_TEST"
		} else {
			l.exit(i, "TYPE_ASSERTION")
		}
		d.Payload = typeTest(l.typ(x.AssertedType), x.CommaOk)
	case *ssa.MakeClosure:
		d.Opcode = "MAKE_CLOSURE"
	case *ssa.Convert:
		l.convert(d, i, x.X.Type(), x.Type())
	case *ssa.ChangeType:
		l.convert(d, i, x.X.Type(), x.Type())
	case *ssa.Call:
		if b, ok := x.Common().Value.(*ssa.Builtin); ok {
			switch b.Name() {
			case "len":
				kind := l.types[l.typ(x.Common().Args[0].Type())].Kind
				if kind == "ARRAY" || kind == "SLICE" || kind == "STRING" {
					d.Opcode = "LENGTH"
				} else {
					l.opaque(d, i, "builtin len")
				}
			case "recover":
				d.Opcode = "RECOVER"
				l.diag("RECOVER_UNMODELED", "INVENTORY", "recover", i.Pos())
			default:
				l.opaque(d, i, "builtin "+b.Name())
			}
			if b.Name() == "close" {
				l.exit(i, "CLOSED_CHANNEL")
			}
		} else {
			l.call(d, x, "CALL")
		}
	case *ssa.Go:
		l.call(d, x, "SPAWN")
		l.diag("CONCURRENCY_PARTIAL", "CONCURRENCY", "goroutine", i.Pos())
	case *ssa.Defer:
		l.call(d, x, "DEFER_PUSH")
		l.diag("DEFER_UNMODELED", "EFFECTS", "defer", i.Pos())
	case *ssa.RunDefers:
		d.Opcode = "RUN_DEFERS"
		l.diag("DEFER_UNMODELED", "EFFECTS", "defer", i.Pos())
	case *ssa.Send:
		d.Opcode = "CHANNEL_SEND"
		l.exit(i, "CLOSED_CHANNEL")
		l.diag("CONCURRENCY_PARTIAL", "CONCURRENCY", "channel send", i.Pos())
	case *ssa.Select:
		l.opaque(d, i, "select")
		l.diag("CONCURRENCY_PARTIAL", "CONCURRENCY", "select", i.Pos())
	case *ssa.MapUpdate:
		l.opaque(d, i, "MapUpdate")
		l.exit(i, "NIL_MAP_WRITE")
	case *ssa.Slice:
		l.opaque(d, i, "Slice")
		l.exit(i, "INDEX_OUT_OF_RANGE")
	default:
		l.opaque(d, i, reflect.TypeOf(i).Elem().Name())
	}
	if d.Opcode == "" {
		l.opaque(d, i, reflect.TypeOf(i).Elem().Name())
	}
	l.Records = append(l.Records, &cir.Record{Content: &cir.Record_InstructionDef{InstructionDef: d}})
	if d.IsTerminator {
		for k, s := range i.Block().Succs {
			e := &cir.ControlEdgeDef{Id: fmt.Sprintf("%s/e%d", d.Id, k), SourceBlockId: bid, TargetBlockId: fmt.Sprintf("%s/b%d", l.FunctionIDs[l.fn], s.Index), TerminatorInstructionId: d.Id, OutcomeOrdinal: uint32(k), EdgeKind: "UNCONDITIONAL", OutcomeLabel: "NEXT"}
			if d.Opcode == "BRANCH" {
				e.GuardValueId = d.OperandValueIds[0]
				if k == 0 {
					e.EdgeKind = "BRANCH_TRUE"
					e.OutcomeLabel = "TRUE"
				} else {
					e.EdgeKind = "BRANCH_FALSE"
					e.OutcomeLabel = "FALSE"
					e.GuardNegated = true
				}
			}
			l.Records = append(l.Records, &cir.Record{Content: &cir.Record_ControlEdgeDef{ControlEdgeDef: e}})
		}
		if x, ok := i.(*ssa.If); ok {
			l.Branches = append(l.Branches, Branch{l.fn, x, d.Id + "/e0", d.Id + "/e1"})
		}
	}
	return count
}
func memory(op string) *cir.OperationPayload {
	return &cir.OperationPayload{Details: &cir.OperationPayload_Memory{Memory: &cir.MemorySemantics{AccessKind: op}}}
}
func field(n int) *cir.OperationPayload {
	return &cir.OperationPayload{Details: &cir.OperationPayload_Field{Field: &cir.FieldAccess{FieldIndex: uint32(n)}}}
}
func typeTest(id string, comma bool) *cir.OperationPayload {
	return &cir.OperationPayload{Details: &cir.OperationPayload_TypeTest{TypeTest: &cir.TypeTest{TargetTypeId: id, CommaOk: comma}}}
}
func (l *lowerer) convert(d *cir.InstructionDef, i ssa.Instruction, from, to types.Type) {
	a, b := l.types[l.typ(from)], l.types[l.typ(to)]
	if a.Kind != "BITVECTOR" || b.Kind != "BITVECTOR" {
		l.opaque(d, i, "conversion")
		return
	}
	switch {
	case a.BitWidth > b.BitWidth:
		d.Opcode = "TRUNCATE"
	case a.BitWidth == b.BitWidth:
		d.Opcode = "BITCAST"
	case a.IsSigned:
		d.Opcode = "SIGN_EXTEND"
	default:
		d.Opcode = "ZERO_EXTEND"
	}
}
func (l *lowerer) call(d *cir.InstructionDef, site ssa.CallInstruction, opcode string) {
	if b, ok := site.Common().Value.(*ssa.Builtin); ok {
		l.opaque(d, site, "builtin "+b.Name())
		return
	}
	r := l.resolver.Resolve(site)
	// Defer.Operands also contains the runtime defer stack, which is not a
	// source argument. Preserve only the callable and its ordered arguments.
	d.OperandValueIds = []string{l.value(site.Common().Value)}
	for _, arg := range site.Common().Args {
		d.OperandValueIds = append(d.OperandValueIds, l.value(arg))
	}
	d.Opcode = opcode
	d.Payload = &cir.OperationPayload{Details: &cir.OperationPayload_Call{Call: &cir.CallSemantics{DispatchKind: r.Dispatch, WorldClosed: r.Closed}}}
	if r.Kind == "CHA" {
		l.diag("CALL_TARGET_APPROX", "CALLGRAPH", "CHA candidates; world remains open", site.Pos())
	}
	if len(r.Targets) == 0 {
		l.diag("CALL_TARGET_UNRESOLVED", "CALLGRAPH", "no call target", site.Pos())
	}
	seenTargets := map[string]bool{}
	for _, f := range r.Targets {
		if seenTargets[l.FunctionIDs[f]] {
			continue
		}
		seenTargets[l.FunctionIDs[f]] = true
		l.Records = append(l.Records, &cir.Record{Content: &cir.Record_CallTargetDef{CallTargetDef: &cir.CallTargetDef{CallInstructionId: d.Id, TargetFunctionId: l.FunctionIDs[f], ResolutionKind: r.Kind}}})
		if !l.local(f) || len(f.Blocks) == 0 {
			l.diag("EXTERNAL_CALL_OPAQUE", "EFFECTS", qualified(f), site.Pos())
		}
	}
}
