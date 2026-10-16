package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	adapter "ske/adapters/go/gen/adapter/v1"
	"ske/adapters/go/internal/emit"
	"ske/adapters/go/internal/frame"
	"ske/adapters/go/internal/load"
	"ske/adapters/go/internal/profile"
)

func run() error {
	var request adapter.AdapterRequest
	if err := frame.Read(os.Stdin, &request, frame.DefaultLimit); err != nil {
		return err
	}
	if request.ProtocolMajor != 1 {
		return fmt.Errorf("PROTOCOL_VERSION: expected protocol_major=1")
	}
	var root, text, key string
	var limits *adapter.Limits
	switch b := request.Body.(type) {
	case *adapter.AdapterRequest_ListUnits:
		root, text, limits = b.ListUnits.ProjectRoot, b.ListUnits.BuildProfileJson, b.ListUnits.Limits
	case *adapter.AdapterRequest_ExtractUnit:
		root, text, key, limits = b.ExtractUnit.ProjectRoot, b.ExtractUnit.BuildProfileJson, b.ExtractUnit.UnitKey, b.ExtractUnit.Limits
	default:
		return fmt.Errorf("REQUEST_INVALID: missing body")
	}
	p, err := profile.Parse(text)
	if err != nil {
		return fmt.Errorf("BUILD_PROFILE_INVALID: %w", err)
	}
	timeout := 10 * time.Minute
	if limits.GetTimeoutMs() > 0 {
		timeout = time.Duration(limits.GetTimeoutMs()) * time.Millisecond
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	project, err := load.Open(ctx, root, p)
	if err != nil {
		return err
	}
	var messages uint64
	write := func(r *adapter.AdapterResponse) error {
		messages++
		if limits.GetMaxMessages() > 0 && messages > limits.GetMaxMessages() {
			return fmt.Errorf("ADAPTER_LIMIT: max messages")
		}
		return frame.Write(os.Stdout, r, limits.GetMaxMessageBytes())
	}
	for _, pkg := range project.Packages {
		unitKey := project.UnitKey(pkg)
		if request.GetListUnits() != nil {
			var paths []string
			for _, path := range pkg.GoFiles {
				rel, e := filepath.Rel(root, path)
				if e != nil {
					return e
				}
				paths = append(paths, filepath.ToSlash(rel))
			}
			if err := write(&adapter.AdapterResponse{Body: &adapter.AdapterResponse_Unit{Unit: &adapter.UnitDescriptor{UnitKey: unitKey, Language: "go", SourcePaths: paths}}}); err != nil {
				return err
			}
		} else if key == unitKey {
			records, e := emit.Extract(project, pkg)
			if e != nil {
				return e
			}
			for _, r := range records {
				if e := write(&adapter.AdapterResponse{Body: &adapter.AdapterResponse_Record{Record: r}}); e != nil {
					return e
				}
			}
			return nil
		}
	}
	if request.GetListUnits() != nil {
		return write(&adapter.AdapterResponse{Body: &adapter.AdapterResponse_ListDone{ListDone: &adapter.ListUnitsDone{}}})
	}
	return fmt.Errorf("UNIT_NOT_FOUND: %s", key)
}

func main() {
	if err := run(); err != nil {
		code := "ADAPTER_ERROR"
		if prefix, _, ok := strings.Cut(err.Error(), ":"); ok && !strings.ContainsAny(prefix, " /\n") {
			code = prefix
		}
		_ = frame.Write(os.Stdout, &adapter.AdapterResponse{Body: &adapter.AdapterResponse_Error{Error: &adapter.AdapterError{Code: code, Message: err.Error()}}}, frame.DefaultLimit)
		_ = json.NewEncoder(os.Stderr).Encode(map[string]string{"severity": "ERROR", "code": code, "message": err.Error()})
		os.Exit(1)
	}
}
