// Rounds and games of the tournament form (templates/tournaments/form.html):
// add / remove rows, keep the shown numbers in order, and show the team
// result fields only for a team event. The field names keep their indices
// (rounds-<i>-…, rounds-<i>-games-<j>-…): the server numbers the rows in the
// order they come (backend/src/routes/tournaments/form.rs).

const form = /** @type {HTMLFormElement} */ (document.getElementById('tournament-form'));
const rounds = /** @type {HTMLElement} */ (document.getElementById('rounds'));
const addRoundButton = /** @type {HTMLButtonElement} */ (document.getElementById('add-round'));
const roundTemplate = /** @type {HTMLTemplateElement} */ (document.getElementById('round-template'));
const gameTemplate = /** @type {HTMLTemplateElement} */ (document.getElementById('game-template'));
const teamSwitch = /** @type {HTMLInputElement} */ (document.getElementById('is_team'));

let nextRound = Number(rounds.dataset.next);

// ---------- numbers ----------

/** "Round 1", "Round 2"… and in each round "Game 1", "Game 2"…, in page order. */
function renumber() {
    rounds.querySelectorAll('.round').forEach((round, i) => {
        /** @type {Element} */ (round.querySelector('.round-number')).textContent = /** @type {any} */ (i + 1);

        round.querySelectorAll('.game-number').forEach((number, j) => {
            number.textContent = `Game ${j + 1}`;
        });
    });
}

// ---------- rows ----------

function addRound() {
    const html = roundTemplate.innerHTML.replaceAll('__r__', /** @type {any} */ (nextRound++));
    rounds.insertAdjacentHTML('beforeend', html);
    renumber();

    /** @type {HTMLSelectElement} */ (/** @type {Element} */ (rounds.lastElementChild).querySelector('select')).focus();
}

function addGame(round) {
    const index = Number(round.dataset.nextGame);
    round.dataset.nextGame = index + 1;

    const html = gameTemplate.innerHTML
        .replaceAll('__r__', round.dataset.index)
        .replaceAll('__g__', /** @type {any} */ (index));
    round.querySelector('.games').insertAdjacentHTML('beforeend', html);
    renumber();
}

addRoundButton.addEventListener('click', addRound);

rounds.addEventListener('click', (e) => {
    const target = /** @type {HTMLElement} */ (e.target);

    if (target.classList.contains('add-game')) {
        addGame(/** @type {HTMLElement} */ (target.closest('.round')));
    } else if (target.classList.contains('remove-game')) {
        /** @type {Element} */ (target.closest('.game')).remove();
        renumber();
    } else if (target.classList.contains('remove-round')) {
        /** @type {Element} */ (target.closest('.round')).remove();
        renumber();
    }
});

// ---------- team event ----------

teamSwitch.addEventListener('change', () => {
    form.classList.toggle('is-team', teamSwitch.checked);
});
