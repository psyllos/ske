package decisions

import (
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"path/filepath"
	"reflect"
	"strconv"
	"testing"
)

func inventory(t *testing.T, fset *token.FileSet, filename string, src any) []Function {
	t.Helper()
	file, err := parser.ParseFile(fset, filename, src, 0)
	if err != nil {
		t.Fatal(err)
	}
	var functions []Function
	ast.Inspect(file, func(n ast.Node) bool {
		switch f := n.(type) {
		case *ast.FuncDecl:
			functions = append(functions, Function{filename + "." + f.Name.Name, f})
		case *ast.FuncLit:
			functions = append(functions, Function{filename + ".closure" + strconv.Itoa(int(f.Pos())), f})
		}
		return true
	})
	return functions
}

func TestCounterMatchesEmitter(t *testing.T) {
	cases := []string{
		`func f(a,b,c bool) { if !(a || (b && c)) {} else if a {} ; for a && !b {} }`,
		`func f(a,b,c bool) bool { x := (a && b) || c; _ = x; return !(a && (b || c)) }`,
		`func f(a,b bool) { if check(a && b) && a {} ; go check(a || b); defer check(a && b) }`,
		`func f(v interface{}, ch chan bool) { switch v {default:;case 1,2:;case 3:}; switch {case true:}; switch x:=v.(type) {case int: _=x}; switch v.(type) {}; select {default:;case <-ch:}; select {}; for {}; for range ch {} }`,
		`func f(a,b bool) { x:=func(){if a && b {}}; _=x; if false {} }`,
		`func f[T any](a,b bool) { if a || b {} }`,
	}
	expected := []uint64{8, 4, 6, 7, 4, 3}
	for i, src := range cases {
		t.Run(strconv.Itoa(i), func(t *testing.T) {
			fset := token.NewFileSet()
			functions := inventory(t, fset, "test.go", "package p\n"+src)
			if got := Count(functions); got != expected[i] {
				t.Fatalf("independent expected total: got %d want %d", got, expected[i])
			}
			functions = append(functions, Function{"generic-instance", functions[0].Node})
			for _, fn := range functions {
				got := Count([]Function{fn})
				want := len(Emit(fset, ".", []Function{fn}))
				if got != uint64(want) {
					t.Fatalf("%s: count=%d emit=%d", fn.ID, got, want)
				}
			}
		})
	}
	t.Run("lab", func(t *testing.T) {
		root := filepath.Join("..", "..", "..", "..", "examples", "go-branch-lab")
		fset := token.NewFileSet()
		files := 0
		err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() || filepath.Ext(path) != ".go" {
				return nil
			}
			files++
			for _, fn := range inventory(t, fset, path, nil) {
				if got, want := Count([]Function{fn}), len(Emit(fset, root, []Function{fn})); got != uint64(want) {
					t.Errorf("%s: count=%d emit=%d", fn.ID, got, want)
				}
			}
			return nil
		})
		if err != nil {
			t.Fatal(err)
		}
		if files == 0 {
			t.Fatal("no lab sources parsed")
		}
	})
}

func TestEmissionContract(t *testing.T) {
	src := "package p\nfunc f(a,b,c bool) { if a || (b && c) {}; switch {default:;case a,b:;case c:}; select {} }\n"
	fset := token.NewFileSet()
	functions := inventory(t, fset, "test.go", src)
	ds := Emit(fset, ".", functions)
	kinds := []string{"IF", "LOGICAL_OPERAND", "LOGICAL_OPERAND", "LOGICAL_OPERAND", "SWITCH", "SELECT"}
	texts := []string{"a || (b && c)", "a", "b", "c", "switch", "select"}
	if len(ds) != len(kinds) {
		t.Fatalf("got %d decisions", len(ds))
	}
	lines := []int{0}
	for i, c := range []byte(src) {
		if c == '\n' {
			lines = append(lines, i+1)
		}
	}
	for i, d := range ds {
		r := d.Record
		if r.DecisionKind != kinds[i] {
			t.Errorf("%d: kind %s", i, r.DecisionKind)
		}
		s := r.Span
		text := src[lines[s.StartLine-1]+int(s.StartColumn)-1 : lines[s.EndLine-1]+int(s.EndColumn)-1]
		if text != texts[i] {
			t.Errorf("%d: span text %q", i, text)
		}
		if r.Id != functions[0].ID+"/sd"+strconv.Itoa(i) {
			t.Errorf("bad id %s", r.Id)
		}
		if i >= 1 && i <= 3 && r.ParentDecisionId != ds[0].Record.Id {
			t.Error("missing logical parent")
		}
		for j, o := range r.Outcomes {
			if o.Id != r.Id+"/o"+strconv.Itoa(j) || o.Elided || o.ActivationMappingComplete {
				t.Error("bad outcome")
			}
		}
	}
	var labels []string
	for _, o := range ds[4].Record.Outcomes {
		labels = append(labels, o.Label)
	}
	if !reflect.DeepEqual(labels, []string{"CASE:0", "CASE:1", "DEFAULT"}) {
		t.Fatal(labels)
	}
	if len(ds[5].Record.Outcomes) != 0 {
		t.Fatal("blocking select has outcomes")
	}
}
