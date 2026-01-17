package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	styx "github.com/bearcove/styx/implementations/styx-go"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "Usage: styx-compliance <corpus-directory>")
		os.Exit(1)
	}

	corpusPath := os.Args[1]
	info, err := os.Stat(corpusPath)
	if err != nil || !info.IsDir() {
		fmt.Fprintf(os.Stderr, "Error: %s is not a directory\n", corpusPath)
		os.Exit(1)
	}

	var styxFiles []string
	err = filepath.Walk(corpusPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if !info.IsDir() && strings.HasSuffix(path, ".styx") {
			styxFiles = append(styxFiles, path)
		}
		return nil
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error walking directory: %v\n", err)
		os.Exit(1)
	}

	sort.Strings(styxFiles)

	var results []string
	for _, path := range styxFiles {
		result := processFile(path, corpusPath)
		results = append(results, result)
	}

	fmt.Println(strings.Join(results, "\n"))
}

func processFile(path, corpusRoot string) string {
	// Get parent directory name for "compliance/corpus/..."
	corpusParent := filepath.Dir(corpusRoot)
	relative := filepath.Join(filepath.Base(corpusParent), filepath.Base(corpusRoot), mustRelPath(corpusRoot, path))

	content, err := os.ReadFile(path)
	if err != nil {
		return fmt.Sprintf("; file: %s\n(error [0, 0] \"read error: %s\")", relative, err)
	}

	doc, parseErr := styx.Parse(string(content))
	if parseErr != nil {
		if pe, ok := parseErr.(*styx.ParseError); ok {
			return fmt.Sprintf("; file: %s\n%s", relative, formatError(pe))
		}
		return fmt.Sprintf("; file: %s\n(error [0, 0] \"parse error: %s\")", relative, parseErr)
	}

	return fmt.Sprintf("; file: %s\n%s", relative, formatDocument(doc))
}

func mustRelPath(base, target string) string {
	rel, err := filepath.Rel(base, target)
	if err != nil {
		return target
	}
	return rel
}

func escapeString(s string) string {
	s = strings.ReplaceAll(s, "\\", "\\\\")
	s = strings.ReplaceAll(s, "\"", "\\\"")
	s = strings.ReplaceAll(s, "\n", "\\n")
	s = strings.ReplaceAll(s, "\r", "\\r")
	s = strings.ReplaceAll(s, "\t", "\\t")
	return s
}

func formatValue(value *styx.Value, indent int) string {
	prefix := strings.Repeat("  ", indent)

	// Unit value (no tag, no payload)
	if value.Tag == nil && value.PayloadKind == styx.PayloadNone {
		return fmt.Sprintf("(unit [%d, %d])", value.Span.Start, value.Span.End)
	}

	// Tag only (no payload)
	if value.Tag != nil && value.PayloadKind == styx.PayloadNone {
		return fmt.Sprintf("(tag [%d, %d] \"%s\")", value.Span.Start, value.Span.End, value.Tag.Name)
	}

	// Tag with payload
	if value.Tag != nil && value.PayloadKind != styx.PayloadNone {
		payloadStr := formatPayload(value, indent+1)
		return fmt.Sprintf("(tag [%d, %d] \"%s\"\n%s  %s)", value.Span.Start, value.Span.End, value.Tag.Name, prefix, payloadStr)
	}

	// Just payload
	if value.PayloadKind != styx.PayloadNone {
		return formatPayload(value, indent)
	}

	return fmt.Sprintf("(unit [%d, %d])", value.Span.Start, value.Span.End)
}

func formatPayload(value *styx.Value, indent int) string {
	prefix := strings.Repeat("  ", indent)

	switch value.PayloadKind {
	case styx.PayloadScalar:
		escaped := escapeString(value.Scalar.Text)
		return fmt.Sprintf("(scalar [%d, %d] %s \"%s\")", value.Scalar.Span.Start, value.Scalar.Span.End, value.Scalar.Kind, escaped)

	case styx.PayloadSequence:
		seq := value.Sequence
		if len(seq.Items) == 0 {
			return fmt.Sprintf("(sequence [%d, %d])", seq.Span.Start, seq.Span.End)
		}
		var items []string
		for _, item := range seq.Items {
			items = append(items, fmt.Sprintf("%s  %s", prefix, formatValue(item, indent+1)))
		}
		return fmt.Sprintf("(sequence [%d, %d]\n%s)", seq.Span.Start, seq.Span.End, strings.Join(items, "\n"))

	case styx.PayloadObject:
		obj := value.Object
		if len(obj.Entries) == 0 {
			return fmt.Sprintf("(object [%d, %d] %s)", obj.Span.Start, obj.Span.End, obj.Separator)
		}
		var entries []string
		for _, entry := range obj.Entries {
			entries = append(entries, formatEntry(entry, indent+1))
		}
		return fmt.Sprintf("(object [%d, %d] %s\n%s\n%s)", obj.Span.Start, obj.Span.End, obj.Separator, strings.Join(entries, "\n"), prefix)
	}

	return "(unknown)"
}

func formatEntry(entry *styx.Entry, indent int) string {
	prefix := strings.Repeat("  ", indent)
	keyStr := formatValue(entry.Key, indent+1)
	valueStr := formatValue(entry.Value, indent+1)
	return fmt.Sprintf("%s(entry\n%s  %s\n%s  %s)", prefix, prefix, keyStr, prefix, valueStr)
}

func formatDocument(doc *styx.Document) string {
	if len(doc.Entries) == 0 {
		return "(document [-1, -1]\n)"
	}
	var entries []string
	for _, entry := range doc.Entries {
		entries = append(entries, formatEntry(entry, 1))
	}
	return fmt.Sprintf("(document [-1, -1]\n%s\n)", strings.Join(entries, "\n"))
}

func formatError(err *styx.ParseError) string {
	escapedMsg := strings.ReplaceAll(err.Message, "\\", "\\\\")
	return fmt.Sprintf("(error [%d, %d] \"parse error at %d-%d: %s\")", err.Span.Start, err.Span.End, err.Span.Start, err.Span.End, escapedMsg)
}
