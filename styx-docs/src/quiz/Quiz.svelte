<script lang="ts">
    import { parseTyped } from "@bearcove/styx";
    import { render_markdown } from "@bearcove/styx-webmd";
    import quizQuestionsStyx from "./quiz-questions.styx?raw";
    import quizQuestionsSchemaStyx from "./quiz-questions.schema.styx?raw";

    interface Option {
        text: string;
        correct: boolean;
        help?: string;
    }

    interface Question {
        question: string;
        options: Option[];
        explanation: string;
    }

    interface QuizData {
        questions: Record<string, Question>;
    }

    interface Props {
        questionId: string;
    }

    let { questionId }: Props = $props();

    let question: Question | null = $state(null);
    let selectedIndex: number | null = $state(null);
    let renderedQuestion: string = $state("");
    let renderedExplanation: string = $state("");
    let renderedHelp: string = $state("");

    async function loadQuestion() {
        const data = parseTyped<QuizData>(quizQuestionsStyx, quizQuestionsSchemaStyx);
        question = data.questions[questionId] ?? null;

        if (!question) {
            console.error(`Question '${questionId}' not found`);
        } else {
            renderedQuestion = await render_markdown(question.question);
        }
    }

    async function selectOption(index: number) {
        if (selectedIndex !== null || !question) return;
        selectedIndex = index;

        const selected = question.options[index];
        renderedExplanation = await render_markdown(question.explanation);

        if (!selected.correct && selected.help) {
            renderedHelp = await render_markdown(selected.help);
        }
    }

    $effect(() => {
        loadQuestion();
    });

    let answered = $derived(selectedIndex !== null);
    let selectedOption = $derived(
        question && selectedIndex !== null ? question.options[selectedIndex] : null,
    );
    let isCorrect = $derived(selectedOption?.correct ?? false);
</script>

<div class="quiz" class:answered>
    {#if !question}
        <div class="loading">Loading...</div>
    {:else}
        <div class="question-container">
            <div class="question-text">
                {@html renderedQuestion}
            </div>

            <div class="options">
                {#each question.options as option, i}
                    <button
                        class="option"
                        class:selected={selectedIndex === i}
                        class:correct={answered && option.correct}
                        class:wrong={selectedIndex === i && !option.correct}
                        onclick={() => selectOption(i)}
                        disabled={answered}
                    >
                        <span class="option-letter">{String.fromCharCode(65 + i)}</span>
                        <span class="option-text">{option.text}</span>
                    </button>
                {/each}
            </div>

            {#if answered}
                <div class="result" class:correct={isCorrect} class:wrong={!isCorrect}>
                    <div class="verdict">{isCorrect ? "Correct!" : "Incorrect."}</div>
                    {#if !isCorrect && renderedHelp}
                        <div class="help">
                            {@html renderedHelp}
                        </div>
                    {/if}
                    <div class="explanation">
                        {@html renderedExplanation}
                    </div>
                </div>
            {/if}
        </div>
    {/if}
</div>

<style>
    .quiz {
        color-scheme: light dark;
        margin: 1.5rem 0;
        padding: 1.25rem;
        border: 1px solid light-dark(#ddd, #333);
        border-radius: 8px;
        background: light-dark(#f5f5f5, #1a1a1a);
    }

    .loading {
        color: light-dark(#666, #888);
    }

    .question-text {
        margin-bottom: 1rem;
        color: light-dark(#1a1a1a, #e5e5e5);
    }

    .question-text :global(p) {
        margin: 0 0 0.75rem;
    }

    .question-text :global(p:last-child) {
        margin-bottom: 0;
    }

    .question-text > :global(.code-block) {
        background: light-dark(#fafafa, #0a0a0a);
        border: 1px solid light-dark(#e0e0e0, #2a2a2a);
        border-radius: 8px;
        overflow: hidden;
        margin: 0.75rem 0;
    }

    .question-text > :global(.code-block) > :global(.code-header) {
        background: light-dark(#f0f0f0, #1a1a1a);
        border-bottom: 1px solid light-dark(#e0e0e0, #2a2a2a);
        padding: 0.4rem 1rem;
        font-size: 0.7rem;
        font-weight: 500;
        font-family: "SF Mono", Monaco, Consolas, monospace;
        color: light-dark(#888, #666);
        text-transform: uppercase;
        letter-spacing: 0.05em;
    }

    /* Compare blocks */
    .question-text :global(.compare-container) {
        display: flex;
        margin: 0.75rem 0;
        border-radius: 8px;
        overflow: hidden;
        border: 1px solid light-dark(#e0e0e0, #2a2a2a);
    }

    .question-text :global(.compare-section) {
        flex: 1;
        background: light-dark(#fafafa, #0a0a0a);
    }

    .question-text :global(.compare-section:not(:last-child)) {
        border-right: 1px solid light-dark(#e0e0e0, #2a2a2a);
    }

    .question-text :global(.compare-header) {
        background: light-dark(#f0f0f0, #1a1a1a);
        border-bottom: 1px solid light-dark(#e0e0e0, #2a2a2a);
        padding: 0.4rem 1rem;
        font-size: 0.7rem;
        font-weight: 500;
        font-family: "SF Mono", Monaco, Consolas, monospace;
        color: light-dark(#888, #666);
        text-transform: uppercase;
        letter-spacing: 0.05em;
    }

    .question-text :global(.compare-section) :global(.code-block) {
        background: none;
        border: none;
        border-radius: 0;
        margin: 0;
    }

    .question-text :global(pre) {
        background: none;
        border: none;
        border-radius: 0;
        padding: 1rem;
        overflow-x: auto;
        margin: 0;
    }

    .question-text :global(code) {
        font-family: "SF Mono", Monaco, Consolas, monospace;
        font-size: 1em;
    }

    .question-text :global(p code) {
        background: light-dark(rgba(0, 0, 0, 0.06), rgba(255, 255, 255, 0.1));
        padding: 0.1em 0.3em;
        border-radius: 3px;
    }

    /* Hide syntax highlighting for Styx code until answer is chosen */
    .quiz:not(.answered) .question-text :global(code.language-styx) {
        /* Core tags */
        :global(a-k),
        :global(a-f),
        :global(a-s),
        :global(a-c),
        :global(a-t),
        :global(a-v),
        :global(a-co),
        :global(a-n),
        :global(a-o),
        :global(a-p),
        :global(a-pr),
        :global(a-at),
        :global(a-tg),
        :global(a-m),
        :global(a-l),
        :global(a-ns),
        :global(a-cr),
        /* Markup tags */
        :global(a-tt),
        :global(a-st),
        :global(a-em),
        :global(a-tu),
        :global(a-tl),
        :global(a-tx),
        /* Diff & special */
        :global(a-da),
        :global(a-dd),
        :global(a-eb),
        :global(a-er) {
            color: inherit !important;
        }
    }

    .options {
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .option {
        display: flex;
        align-items: flex-start;
        gap: 0.75rem;
        padding: 0.875rem 1rem;
        border: 2px solid light-dark(#ddd, #333);
        border-radius: 8px;
        background: light-dark(#fff, #161616);
        color: light-dark(#1a1a1a, #e5e5e5);
        font-size: 0.9rem;
        text-align: left;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    .option:hover:not(:disabled) {
        border-color: light-dark(#aaa, #4a4a4a);
        background: light-dark(#f5f5f5, #1e1e1e);
        transform: translateX(2px);
    }

    .option:disabled {
        cursor: default;
    }

    .option:disabled:not(.correct):not(.wrong) {
        opacity: 0.6;
    }

    .option.correct {
        border-color: light-dark(#22c55e, #4ade80);
        background: rgba(74, 222, 128, 0.15);
    }

    .option.wrong {
        border-color: light-dark(#ef4444, #f87171);
        background: rgba(248, 113, 113, 0.15);
    }

    .option-letter {
        flex-shrink: 0;
        width: 1.5rem;
        height: 1.5rem;
        display: flex;
        align-items: center;
        justify-content: center;
        border-radius: 50%;
        background: light-dark(#e5e5e5, #333);
        font-weight: 600;
        font-size: 0.8rem;
    }

    .option.correct .option-letter {
        background: light-dark(#22c55e, #4ade80);
        color: white;
    }

    .option.wrong .option-letter {
        background: light-dark(#ef4444, #f87171);
        color: white;
    }

    .option-text {
        flex: 1;
        line-height: 1.5;
    }

    .result {
        margin-top: 1rem;
        padding: 1rem;
        border-radius: 6px;
        font-size: 0.9rem;
        line-height: 1.5;
    }

    .result.correct {
        background: rgba(74, 222, 128, 0.1);
        border: 1px solid rgba(74, 222, 128, 0.3);
        color: light-dark(#166534, #a7f3d0);
    }

    .result.wrong {
        background: rgba(248, 113, 113, 0.1);
        border: 1px solid rgba(248, 113, 113, 0.3);
        color: light-dark(#991b1b, #fecaca);
    }

    .verdict {
        font-weight: 700;
        font-size: 1rem;
        margin-bottom: 0.75rem;
    }

    .help {
        padding: 0.75rem;
        margin-bottom: 0.75rem;
        background: rgba(0, 0, 0, 0.1);
        border-radius: 4px;
        font-style: italic;
    }

    .help :global(p:last-child) {
        margin-bottom: 0;
    }

    .explanation :global(p) {
        margin: 0 0 0.5rem;
    }

    .explanation :global(p:last-child) {
        margin-bottom: 0;
    }

    .explanation :global(.code-block) {
        margin: 0.75rem 0;
    }

    .explanation :global(.code-block:last-child) {
        margin-bottom: 0;
    }

    .explanation :global(code),
    .help :global(code) {
        padding: 0.1em 0.3em;
        border-radius: 3px;
        font-family: "SF Mono", Monaco, Consolas, monospace;
        font-size: 1em;
    }

    .explanation :global(ul),
    .explanation :global(ol) {
        margin: 0.5rem 0;
        padding-left: 1.5rem;
    }

    .explanation :global(li) {
        margin: 0.25rem 0;
    }
</style>
