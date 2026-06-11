// Runtime payload traversal is deeper than schema traversal for recursive DTOs:
// one logical value edge may visit struct fields, list containers, and call blocks.
export const MESSAGE_MAX_DEPTH = 1024;
