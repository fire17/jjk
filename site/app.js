const demos = {
  save: {
    terminal: `jjk init
jjk "baseline before parser rewrite"
jjk step "separated state model from git engine"
jjk nice "green tests after cleanup"`,
    copy:
      "Save meaningful moments without rewriting visible branch history. `jjk` stores a hidden snapshot commit and a human-readable state record together.",
  },
  return: {
    terminal: `jjk see
jjk return "green tests after cleanup"
git branch --show-current
# jjk/return-a18c924d`,
    copy:
      "Returning is cheap. Resolve a state by id, label, or description, then switch to a branch rooted at that snapshot instead of reconstructing the past by hand.",
  },
  lane: {
    terminal: `jjk lane feature-harvest
jjk "imported upstream parser attempt"
jjk star "memorable baseline for comparison"`,
    copy:
      "Lanes are named streams of work. They keep experiments legible and make side-by-side attempts easier to reason about.",
  },
  watch: {
    terminal: `jjk watch
# edit files...
saved c3b5d91f auto auto grouped change near src/parser.ts`,
    copy:
      "Watch mode groups nearby changes into `auto` states so the project remembers motion without spamming one save per file event.",
  },
};

const terminal = document.querySelector("#demo-terminal");
const copy = document.querySelector("#demo-copy");
const buttons = [...document.querySelectorAll("[data-demo]")];

function activate(name) {
  const demo = demos[name];
  terminal.textContent = demo.terminal;
  copy.textContent = demo.copy;
  buttons.forEach((button) => {
    button.classList.toggle("active", button.dataset.demo === name);
  });
}

buttons.forEach((button) => {
  button.addEventListener("click", () => activate(button.dataset.demo));
});

activate("save");
