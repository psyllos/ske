package frame

import (
	"bytes"
	"encoding/binary"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"google.golang.org/protobuf/proto"
	cir "ske/adapters/go/gen/cir/v1"
)

func TestRoundTripFixtures(t *testing.T) {
	files, err := filepath.Glob("../../../../tests/fixtures/cir/*.cir.pb")
	if err != nil {
		t.Fatal(err)
	}
	count := 0
	for _, path := range files {
		if strings.HasPrefix(filepath.Base(path), "bad_") {
			continue
		}
		count++
		b, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		r := bytes.NewReader(b)
		var roundtrip bytes.Buffer
		var records []*cir.Record
		for r.Len() > 0 {
			var original cir.Record
			if err := Read(r, &original, 0); err != nil {
				t.Fatal(path, err)
			}
			var out bytes.Buffer
			if err := Write(&out, &original, 0); err != nil {
				t.Fatal(err)
			}
			var decoded cir.Record
			if err := Read(&out, &decoded, 0); err != nil {
				t.Fatal(err)
			}
			if !proto.Equal(&original, &decoded) {
				t.Fatal(path, "semantic roundtrip mismatch")
			}
			if err := Write(&roundtrip, &decoded, 0); err != nil {
				t.Fatal(err)
			}
			records = append(records, &original)
		}
		dir := t.TempDir()
		goPath := filepath.Join(dir, "go.cir.pb")
		rustPath := filepath.Join(dir, "rust.cir.pb")
		if err := os.WriteFile(goPath, roundtrip.Bytes(), 0600); err != nil {
			t.Fatal(err)
		}
		crossLanguage(t, path, goPath, rustPath, true)
		rustBytes, err := os.ReadFile(rustPath)
		if err != nil {
			t.Fatal(err)
		}
		fromRust := bytes.NewReader(rustBytes)
		for _, original := range records {
			var decoded cir.Record
			if err := Read(fromRust, &decoded, 0); err != nil {
				t.Fatal(err)
			}
			if !proto.Equal(original, &decoded) {
				t.Fatal(path, "Rust return-trip changed decoded fields")
			}
		}
		if fromRust.Len() != 0 {
			t.Fatal(path, "extra records after Rust return trip")
		}
	}
	if count != 7 {
		t.Fatalf("expected seven positive fixtures, got %d", count)
	}
}

func crossLanguage(t *testing.T, original, goPath, rustPath string, success bool) {
	t.Helper()
	root, err := filepath.Abs("../../../..")
	if err != nil {
		t.Fatal(err)
	}
	source, err := filepath.Abs(original)
	if err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("cargo", "run", "--quiet", "-p", "ske-cir", "--example",
		"compare_records", "--", source, goPath, rustPath)
	cmd.Dir = root
	output, err := cmd.CombinedOutput()
	if success && err != nil {
		t.Fatalf("Rust semantic comparison failed: %v\n%s", err, output)
	}
	if !success && err == nil {
		t.Fatal("Rust comparison accepted changed optional presence")
	}
	if !success && !bytes.Contains(output, []byte("CIR semantic mismatch")) {
		t.Fatalf("expected a semantic rejection, not a tool failure: %v\n%s", err, output)
	}
}

func TestRustDetectsOptionalPresenceDifference(t *testing.T) {
	dir := t.TempDir()
	write := func(path string, raise *bool) {
		t.Helper()
		record := &cir.Record{Content: &cir.Record_InstructionDef{InstructionDef: &cir.InstructionDef{
			Payload: &cir.OperationPayload{Details: &cir.OperationPayload_Call{Call: &cir.CallSemantics{MayRaise: raise}}},
		}}}
		var out bytes.Buffer
		if err := Write(&out, record, 0); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, out.Bytes(), 0600); err != nil {
			t.Fatal(err)
		}
	}
	absent := filepath.Join(dir, "absent.pb")
	explicit := filepath.Join(dir, "false.pb")
	falseValue := false
	write(absent, nil)
	write(explicit, &falseValue)
	crossLanguage(t, absent, explicit, filepath.Join(dir, "mismatch.pb"), false)
	// Explicit false also survives the Rust return trip, not only Go's.
	returned := filepath.Join(dir, "returned.pb")
	crossLanguage(t, explicit, explicit, returned, true)
	data, err := os.ReadFile(returned)
	if err != nil {
		t.Fatal(err)
	}
	var record cir.Record
	if err := Read(bytes.NewReader(data), &record, 0); err != nil {
		t.Fatal(err)
	}
	call := record.GetInstructionDef().GetPayload().GetCall()
	if call.MayRaise == nil || *call.MayRaise || call.MayMutateMemory != nil {
		t.Fatal("Rust lost optional presence")
	}
}
func TestBoundsAndTruncation(t *testing.T) {
	var prefix [4]byte
	binary.BigEndian.PutUint32(prefix[:], 100)
	if err := Read(bytes.NewReader(prefix[:]), &cir.Record{}, 10); err == nil {
		t.Fatal("oversized frame accepted")
	}
	if err := Read(bytes.NewReader(prefix[:]), &cir.Record{}, 100); err != io.EOF {
		t.Fatal(err)
	}
	if err := Read(bytes.NewReader(prefix[:2]), &cir.Record{}, 100); err != io.ErrUnexpectedEOF {
		t.Fatal(err)
	}
}
func TestPresence(t *testing.T) {
	f := false
	r := &cir.Record{Content: &cir.Record_InstructionDef{InstructionDef: &cir.InstructionDef{Payload: &cir.OperationPayload{Details: &cir.OperationPayload_Call{Call: &cir.CallSemantics{MayRaise: &f}}}}}}
	var out bytes.Buffer
	if err := Write(&out, r, 0); err != nil {
		t.Fatal(err)
	}
	var got cir.Record
	if err := Read(&out, &got, 0); err != nil {
		t.Fatal(err)
	}
	call := got.GetInstructionDef().GetPayload().GetCall()
	if call.MayRaise == nil || *call.MayRaise || call.MayMutateMemory != nil {
		t.Fatal("optional presence lost")
	}
}
