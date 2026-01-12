package tree_sitter_styx_test

import (
	"testing"

	tree_sitter "github.com/smacker/go-tree-sitter"
	"github.com/tree-sitter/tree-sitter-styx"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_styx.Language())
	if language == nil {
		t.Errorf("Error loading Styx grammar")
	}
}
