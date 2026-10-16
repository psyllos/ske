// Package frame implements the bounded, length-prefixed adapter protocol.
package frame

import (
	"encoding/binary"
	"fmt"
	"io"

	"google.golang.org/protobuf/proto"
)

const DefaultLimit = 16 * 1024 * 1024

func Read(r io.Reader, m proto.Message, limit uint64) error {
	if limit == 0 {
		limit = DefaultLimit
	}
	var prefix [4]byte
	if _, err := io.ReadFull(r, prefix[:]); err != nil {
		return err
	}
	n := uint64(binary.BigEndian.Uint32(prefix[:]))
	if n > limit {
		return fmt.Errorf("frame length %d exceeds limit %d", n, limit)
	}
	b := make([]byte, int(n))
	if _, err := io.ReadFull(r, b); err != nil {
		return err
	}
	return proto.Unmarshal(b, m)
}

func Write(w io.Writer, m proto.Message, limit uint64) error {
	b, err := (proto.MarshalOptions{Deterministic: true}).Marshal(m)
	if err != nil {
		return err
	}
	if limit == 0 {
		limit = DefaultLimit
	}
	if uint64(len(b)) > limit || uint64(len(b)) > 0xffffffff {
		return fmt.Errorf("frame exceeds limit")
	}
	var prefix [4]byte
	binary.BigEndian.PutUint32(prefix[:], uint32(len(b)))
	for _, data := range [][]byte{prefix[:], b} {
		for len(data) > 0 {
			n, err := w.Write(data)
			if err != nil {
				return err
			}
			if n == 0 {
				return io.ErrShortWrite
			}
			data = data[n:]
		}
	}
	return nil
}
