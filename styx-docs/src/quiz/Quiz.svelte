<script lang="ts">
    import { parseTyped } from "@bearcove/styx";
    import init, { render_markdown } from "../webmd/styx_webmd.js";

    interface Question {
        code: string;
        valid: boolean;
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
    let selectedAnswer: boolean | null = $state(null);
    let renderedExplanation: string = $state("");

    async function loadQuestion() {
        // Init wasm module
        await init();

        const [dataResponse, schemaResponse] = await Promise.all([
            fetch("/quiz-questions.styx"),
            fetch("/quiz-questions.schema.styx"),
        ]);

        const dataText = await dataResponse.text();
        const schemaText = await schemaResponse.text();

        const data = parseTyped<QuizData>(dataText, schemaText);
        question = data.questions[questionId] ?? null;

        if (!question) {
            console.error(`Question '${questionId}' not found`);
        } else {
            // Pre-render the explanation markdown
            renderedExplanation = await render_markdown(question.explanation);
        }
    }

    function answer(isValid: boolean) {
        if (selectedAnswer !== null) return;
        selectedAnswer = isValid;
    }

    $effect(() => {
        loadQuestion();
    });

    let answered = $derived(selectedAnswer !== null);
    let isCorrect = $derived(answered && selectedAnswer === question?.valid);
</script>

<div class="quiz">
    {#if !question}
        <div class="loading">Loading...</div>
    {:else}
        <div class="question">
            <div class="prompt">Is this valid Styx?</div>
            <pre class="code">{question.code}</pre>

            <div class="buttons">
                <button
                    class="btn valid"
                    class:correct={selectedAnswer === true && question.valid}
                    class:wrong={selectedAnswer === true && !question.valid}
                    onclick={() => answer(true)}
                    disabled={answered}
                >
                    Valid
                </button>
                <button
                    class="btn invalid"
                    class:correct={selectedAnswer === false && !question.valid}
                    class:wrong={selectedAnswer === false && question.valid}
                    onclick={() => answer(false)}
                    disabled={answered}
                >
                    Invalid
                </button>
            </div>

            {#if answered}
                <div class="result" class:correct={isCorrect} class:wrong={!isCorrect}>
                    <span class="verdict">{isCorrect ? "Correct!" : "Incorrect."}</span>
                    {@html renderedExplanation}
                </div>
            {/if}
        </div>
    {/if}
</div>

<style>
    .quiz {
        margin: 1.5rem 0;
        padding: 1rem;
        border: 1px solid light-dark(#ddd, #333);
        border-radius: 8px;
        background: light-dark(#f5f5f5, #1a1a1a);
    }

    .loading {
        color: light-dark(#666, #888);
        font-style: italic;
    }

    .prompt {
        font-weight: 600;
        margin-bottom: 0.75rem;
        color: light-dark(#333, #ccc);
    }

    .code {
        background: light-dark(#fff, #0d0d0d);
        color: light-dark(#1a1a1a, #e5e5e5);
        padding: 0.75rem 1rem;
        border-radius: 6px;
        overflow-x: auto;
        font-family: "SF Mono", Monaco, Consolas, monospace;
        font-size: 0.85rem;
        line-height: 1.5;
        margin: 0 0 1rem;
        border: 1px solid light-dark(#ddd, #2a2a2a);
    }

    .buttons {
        display: flex;
        gap: 0.75rem;
    }

    .btn {
        flex: 1;
        padding: 0.5rem 1rem;
        border: 2px solid light-dark(#ccc, #333);
        border-radius: 6px;
        background: light-dark(#fff, #1e1e1e);
        color: light-dark(#1a1a1a, #e5e5e5);
        font-size: 0.9rem;
        font-weight: 600;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    .btn:hover:not(:disabled) {
        border-color: light-dark(#999, #555);
        background: light-dark(#f0f0f0, #252525);
    }

    .btn:disabled {
        cursor: default;
        opacity: 0.8;
    }

    .btn.correct {
        border-color: light-dark(#22c55e, #4ade80);
        background: rgba(74, 222, 128, 0.15);
    }

    .btn.wrong {
        border-color: light-dark(#ef4444, #f87171);
        background: rgba(248, 113, 113, 0.15);
    }

    .result {
        margin-top: 1rem;
        padding: 0.75rem 1rem;
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
        font-weight: 600;
        margin-right: 0.5rem;
    }
</style>
