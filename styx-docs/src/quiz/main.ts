import { mount } from "svelte";
import Quiz from "./Quiz.svelte";

// Find all quiz placeholders and hydrate them
document.querySelectorAll("[data-quiz]").forEach((el) => {
  const questionId = el.getAttribute("data-quiz");
  if (questionId) {
    mount(Quiz, { target: el, props: { questionId } });
  }
});
