// The live page (/tournaments/:id/live): a round noted in a few taps, kept in
// localStorage at once, sent to the app when the network allows, and written
// as a short line to share in the chat.
//
//   PUT   /api/tournaments/:id/rounds/:number   replaces one round
//   PATCH /api/tournaments/:id                  the end of the day
//
// Nothing is lost by locking the phone or losing the network: every change is
// in the queue (`localStorage['tournament-live-<id>']`) until the app says OK.

(function () {
    /**
     * @typedef {{ result: string, went_first: boolean | null, comment: string }} Game
     * @typedef {{
     *     number: number, opponent_deck: string, labels: string[], die: string,
     *     result: string, team_result: string, comment: string, games: Game[]
     * }} Round
     * @typedef {{ placement: number | null, players: number | null, conclusion: string }} End
     * @typedef {{
     *     id: number, name: string, is_team: boolean, rounds: Round[],
     *     chips: string[], labels: string[], end: End
     * }} LiveData
     * @typedef {{ rounds: Record<string, Round>, end: End | null, draft: Round | null }} Queue
     */

    // The words of the shared line, in one place: switch them to French here.
    const WORDS = {
        round: 'R',
        win: 'W',
        loss: 'L',
        doubleLoss: 'Double loss',
        bye: 'Bye',
        forfeitWin: 'W (forfeit)',
        versus: 'vs',
        dieWon: 'die won',
        dieLost: 'die lost',
        first: '1st',
        second: '2nd',
        team: 'team',
        rounds: 'rounds',
        round1: 'round',
        copied: 'Copied!',
    };

    /** How many chips show before "more". */
    const CHIPS_SHOWN = 8;

    const root = document.getElementById('live');
    const dataElement = document.getElementById('live-data');

    if (!root || !dataElement) {
        return;
    }

    const data = /** @type {LiveData} */ (JSON.parse(dataElement.textContent || 'null'));
    const storageKey = 'tournament-live-' + data.id;

    /** @param {string} id @returns {HTMLElement} */
    function element(id) {
        return /** @type {HTMLElement} */ (document.getElementById(id));
    }

    const ui = {
        record: element('live-record'),
        sync: element('live-sync'),
        problem: element('live-problem'),
        login: element('live-login'),
        retry: element('live-retry'),
        rounds: element('live-rounds'),
        form: /** @type {HTMLFormElement} */ (element('live-form')),
        title: element('live-title'),
        games: element('live-games'),
        result: element('live-result'),
        chips: element('live-chips'),
        label: /** @type {HTMLInputElement} */ (element('live-label')),
        labelAdd: element('live-label-add'),
        labelList: element('live-labels'),
        resultSelect: /** @type {HTMLSelectElement} */ (element('live-result-select')),
        teamSelect: /** @type {HTMLSelectElement | null} */ (document.getElementById('live-team-select')),
        comment: /** @type {HTMLTextAreaElement} */ (element('live-comment')),
        save: element('live-save'),
        shareDay: element('live-share-day'),
        fallback: /** @type {HTMLTextAreaElement} */ (element('live-fallback')),
        endForm: /** @type {HTMLFormElement} */ (element('live-end-form')),
        placement: /** @type {HTMLInputElement} */ (element('live-placement')),
        players: /** @type {HTMLInputElement} */ (element('live-players')),
        conclusion: /** @type {HTMLTextAreaElement} */ (element('live-conclusion')),
        endSync: element('live-end-sync'),
    };

    // ---------- the queue on the phone ----------

    /** @returns {Queue} */
    function loadQueue() {
        try {
            const stored = JSON.parse(localStorage.getItem(storageKey) || 'null');

            if (stored && typeof stored === 'object') {
                return { rounds: stored.rounds || {}, end: stored.end || null, draft: stored.draft || null };
            }
        } catch {
            // an unreadable queue is an empty one
        }

        return { rounds: {}, end: null, draft: null };
    }

    /** @type {Queue} */
    let queue = loadQueue();

    function saveQueue() {
        try {
            localStorage.setItem(storageKey, JSON.stringify(queue));
        } catch {
            // storage full or forbidden: the notes still go to the app
        }
    }

    /** The rounds the app has, by number (what the page started with, then what was sent). */
    /** @type {Map<number, Round>} */
    const known = new Map(data.rounds.map((r) => [r.number, r]));

    /** @type {End} */
    let knownEnd = data.end;

    /** @returns {Round[]} every round, the queued ones over the app's, in order */
    function allRounds() {
        /** @type {Map<number, Round>} */
        const merged = new Map(known);

        for (const round of Object.values(queue.rounds)) {
            merged.set(round.number, round);
        }

        return [...merged.values()].sort((a, b) => a.number - b.number);
    }

    /** @param {number} number */
    function isPending(number) {
        return String(number) in queue.rounds;
    }

    // ---------- reading a round ----------

    /** @param {Round} round @returns {{ won: number, lost: number, draw: number }} */
    function score(round) {
        const count = (/** @type {string} */ result) => round.games.filter((g) => g.result === result).length;

        return { won: count('won'), lost: count('lost'), draw: count('draw') };
    }

    /** The result the games say; '' when they don't decide. @param {Round} round */
    function resultFromGames(round) {
        const { won, lost } = score(round);

        if (won > lost) {
            return 'win';
        }

        return lost > won ? 'loss' : '';
    }

    /** The result as saved: the one picked, else the games'. @param {Round} round */
    function effectiveResult(round) {
        return round.result || resultFromGames(round);
    }

    /** @param {string} result */
    function resultWord(result) {
        switch (result) {
            case 'win':
                return WORDS.win;
            case 'loss':
                return WORDS.loss;
            case 'double_loss':
                return WORDS.doubleLoss;
            case 'bye':
                return WORDS.bye;
            case 'forfeit_win':
                return WORDS.forfeitWin;
            default:
                return '?';
        }
    }

    /** @param {Round} round */
    function opponentText(round) {
        return round.opponent_deck || round.labels.join(' / ');
    }

    /** "2-1", "2-1-1" with draws, '' without games. @param {Round} round */
    function scoreText(round) {
        if (round.games.length === 0) {
            return '';
        }

        const { won, lost, draw } = score(round);

        return draw > 0 ? `${won}-${lost}-${draw}` : `${won}-${lost}`;
    }

    /** The short line to share: "R2: W 2-1 vs Mitsurugi (die won, 2nd)". @param {Round} round */
    function line(round) {
        const result = effectiveResult(round);
        const parts = [resultWord(result)];
        const games = scoreText(round);

        if (games && result !== 'bye') {
            parts.push(games);
        }

        const opponent = opponentText(round);

        if (opponent && result !== 'bye') {
            parts.push(`${WORDS.versus} ${opponent}`);
        }

        /** @type {string[]} */
        const details = [];

        if (round.die === 'won') {
            details.push(WORDS.dieWon);
        } else if (round.die === 'lost') {
            details.push(WORDS.dieLost);
        }

        const first = round.games.find((g) => g.went_first !== null);

        if (first) {
            details.push(first.went_first ? WORDS.first : WORDS.second);
        }

        if (round.team_result) {
            details.push(`${WORDS.team} ${resultWord(round.team_result)}`);
        }

        if (details.length > 0) {
            parts.push(`(${details.join(', ')})`);
        }

        return `${WORDS.round}${round.number}: ${parts.join(' ')}`;
    }

    /** "2-1" or "2-1, 1 bye" over the rounds. @param {Round[]} rounds */
    function record(rounds) {
        let wins = 0;
        let losses = 0;
        let byes = 0;

        for (const round of rounds) {
            const result = effectiveResult(round);

            if (result === 'win' || result === 'forfeit_win') {
                wins += 1;
            } else if (result === 'loss' || result === 'double_loss') {
                losses += 1;
            } else if (result === 'bye') {
                byes += 1;
            }
        }

        if (wins + losses + byes === 0) {
            return '';
        }

        return `${wins}-${losses}` + (byes > 0 ? `, ${byes} ${byes === 1 ? 'bye' : 'byes'}` : '');
    }

    /** All the lines, then the record. */
    function dayText() {
        const rounds = allRounds();
        const lines = rounds.map(line);
        const total = record(rounds);

        if (total) {
            lines.push(`→ ${total} (${rounds.length} ${rounds.length === 1 ? WORDS.round1 : WORDS.rounds})`);
        }

        return lines.join('\n');
    }

    // ---------- the round being noted ----------

    /** @returns {Round} */
    function blankRound() {
        const rounds = allRounds();
        const last = rounds.length > 0 ? rounds[rounds.length - 1].number : 0;

        return {
            number: last + 1,
            opponent_deck: '',
            labels: [],
            die: '',
            result: '',
            team_result: '',
            comment: '',
            games: [],
        };
    }

    /** @type {Round} */
    let current = queue.draft || blankRound();

    /** The draft is kept as it changes. */
    function keepDraft() {
        queue.draft = current;
        saveQueue();
    }

    /** @param {Round} round */
    function copy(round) {
        return /** @type {Round} */ (JSON.parse(JSON.stringify(round)));
    }

    /**
     * @param {string} tag
     * @param {string} [className]
     * @param {string} [text]
     */
    function make(tag, className, text) {
        const node = document.createElement(tag);

        if (className) {
            node.className = className;
        }

        if (text !== undefined) {
            node.textContent = text;
        }

        return node;
    }

    function renderRounds() {
        ui.rounds.replaceChildren();

        for (const round of allRounds()) {
            const item = make('li');
            const button = make('button', 'live-round', '');
            const state = isPending(round.number) ? '⏳' : '✓';
            const result = effectiveResult(round);
            const text = [`R${round.number}`, resultWord(result), scoreText(round), opponentText(round) && `vs ${opponentText(round)}`];
            button.textContent = text.filter(Boolean).join(' ') + ' ' + state;
            button.classList.add(result === 'win' || result === 'forfeit_win' ? 'win' : result === 'bye' ? 'other' : 'loss');
            button.addEventListener('click', () => {
                current = copy(round);
                keepDraft();
                renderForm();
                ui.form.scrollIntoView({ behavior: 'smooth' });
            });
            item.append(button);
            ui.rounds.append(item);
        }

        ui.record.textContent = record(allRounds());
    }

    function renderGames() {
        ui.games.replaceChildren();

        current.games.forEach((game, index) => {
            const chip = make('span', 'live-game ' + game.result);
            const undo = make('button', 'live-game-main', `${game.result === 'won' ? 'W' : game.result === 'lost' ? 'L' : 'D'}`);
            undo.setAttribute('type', 'button');
            undo.title = 'Remove this game';
            undo.addEventListener('click', () => {
                current.games.splice(index, 1);
                keepDraft();
                renderForm();
            });

            const turn = make('button', 'live-game-turn', game.went_first === null ? '1st/2nd?' : game.went_first ? '1st' : '2nd');
            turn.setAttribute('type', 'button');
            turn.addEventListener('click', () => {
                // unknown → 1st → 2nd → unknown
                game.went_first = game.went_first === null ? true : game.went_first ? false : null;
                keepDraft();
                renderGames();
            });

            chip.append(undo, turn);
            ui.games.append(chip);
        });
    }

    function renderResult() {
        const result = effectiveResult(current);
        ui.result.textContent = result
            ? `Result: ${resultWord(result)} ${scoreText(current)}`.trim() + (current.result ? '' : ' (from the games)')
            : current.games.length > 0
              ? 'Pick the result under “Other result…”'
              : 'Tap W or L for each game';
    }

    /** The chips: the most met, then the chosen ones that are not among them. */
    function renderChips(showAll = false) {
        ui.chips.replaceChildren();

        const shown = showAll ? data.chips : data.chips.slice(0, CHIPS_SHOWN);
        const names = [...shown];

        for (const label of current.labels) {
            if (!names.some((n) => n.toLowerCase() === label.toLowerCase())) {
                names.push(label);
            }
        }

        for (const name of names) {
            const chip = make('button', 'live-chip', name);
            chip.setAttribute('type', 'button');
            const on = current.labels.some((l) => l.toLowerCase() === name.toLowerCase());
            chip.classList.toggle('on', on);
            chip.setAttribute('aria-pressed', String(on));
            chip.addEventListener('click', () => toggleLabel(name));
            ui.chips.append(chip);
        }

        if (!showAll && data.chips.length > CHIPS_SHOWN) {
            const more = make('button', 'live-chip more', 'more…');
            more.setAttribute('type', 'button');
            more.addEventListener('click', () => renderChips(true));
            ui.chips.append(more);
        }
    }

    /** @param {string} name */
    function toggleLabel(name) {
        const lower = name.toLowerCase();
        const at = current.labels.findIndex((l) => l.toLowerCase() === lower);

        if (at >= 0) {
            current.labels.splice(at, 1);
        } else {
            current.labels.push(name);
        }

        // one field: the deck as written follows the chips
        current.opponent_deck = current.labels.join(' / ');
        keepDraft();
        renderChips();
    }

    function renderForm() {
        const editing = known.has(current.number) || isPending(current.number);
        ui.title.textContent = (editing ? 'Correct round ' : 'Round ') + current.number;
        renderGames();
        renderResult();
        renderChips();
        ui.resultSelect.value = current.result;

        if (ui.teamSelect) {
            ui.teamSelect.value = current.team_result;
        }

        ui.comment.value = current.comment;

        for (const button of document.querySelectorAll('button[data-die]')) {
            button.classList.toggle('on', button.getAttribute('data-die') === current.die);
        }
    }

    for (const button of document.querySelectorAll('button[data-game]')) {
        button.addEventListener('click', () => {
            current.games.push({ result: button.getAttribute('data-game') || 'won', went_first: null, comment: '' });
            keepDraft();
            renderGames();
            renderResult();
        });
    }

    for (const button of document.querySelectorAll('button[data-die]')) {
        button.addEventListener('click', () => {
            const die = button.getAttribute('data-die') || '';
            // a second tap on the same one clears it
            current.die = current.die === die ? '' : die;
            keepDraft();
            renderForm();
        });
    }

    ui.resultSelect.addEventListener('change', () => {
        current.result = ui.resultSelect.value;
        keepDraft();
        renderResult();
    });

    ui.teamSelect?.addEventListener('change', () => {
        current.team_result = ui.teamSelect ? ui.teamSelect.value : '';
        keepDraft();
    });

    ui.comment.addEventListener('input', () => {
        current.comment = ui.comment.value;
        keepDraft();
    });

    function addTypedLabel() {
        const name = ui.label.value.trim();

        if (!name) {
            return;
        }

        ui.label.value = '';

        if (!current.labels.some((l) => l.toLowerCase() === name.toLowerCase())) {
            toggleLabel(name);
        }
    }

    ui.labelAdd.addEventListener('click', addTypedLabel);
    ui.label.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            event.preventDefault();
            addTypedLabel();
        }
    });

    for (const label of data.labels) {
        const option = document.createElement('option');
        option.value = label;
        ui.labelList.append(option);
    }

    // ---------- saving: queue first, then the network ----------

    /**
     * Put the round being noted in the queue and start the next one.
     * @returns {Round | null} the round queued, null when there is nothing to save
     */
    function queueCurrent() {
        if (!effectiveResult(current)) {
            ui.problem.textContent = current.games.length > 0
                ? 'The games are level: pick the result under “Other result…”.'
                : 'Tap W or L for the games, or pick a result under “Other result…”.';
            ui.problem.hidden = false;

            return null;
        }

        ui.problem.hidden = true;

        const round = copy(current);
        round.result = effectiveResult(round);
        queue.rounds[String(round.number)] = round;
        current = blankRound();
        queue.draft = null;
        saveQueue();
        renderRounds();
        renderForm();

        return round;
    }

    let syncing = false;
    let again = false;

    /** @param {string} url @param {string} method @param {unknown} body */
    async function send(url, method, body) {
        return fetch(url, {
            method,
            headers: { 'Content-Type': 'application/json' },
            credentials: 'same-origin',
            body: JSON.stringify(body),
        });
    }

    /** @param {Round} round */
    function roundBody(round) {
        return {
            opponent_deck: round.opponent_deck,
            labels: round.labels,
            die: round.die,
            result: round.result,
            team_result: round.team_result,
            comment: round.comment,
            games: round.games,
        };
    }

    function showSync() {
        const waiting = Object.keys(queue.rounds).length + (queue.end ? 1 : 0);
        ui.sync.textContent = waiting > 0 ? `⏳ ${waiting} waiting` : '✓ saved';
        ui.retry.hidden = waiting === 0;
        ui.endSync.textContent = queue.end ? '⏳' : '';
    }

    /** Send what waits, in order; stop at the first failure (retried later). */
    async function sync() {
        if (syncing) {
            again = true;

            return;
        }

        syncing = true;
        ui.login.hidden = true;

        try {
            do {
                again = false;
                await syncOnce();
            } while (again);
        } finally {
            syncing = false;
            showSync();
        }
    }

    async function syncOnce() {
        const numbers = Object.keys(queue.rounds)
            .map(Number)
            .sort((a, b) => a - b);

        for (const number of numbers) {
            const round = queue.rounds[String(number)];
            const sent = JSON.stringify(round);

            if (!(await attempt(`/api/tournaments/${data.id}/rounds/${number}`, 'PUT', roundBody(round)))) {
                return;
            }

            known.set(number, round);

            // changed again while it was on its way: it stays queued
            if (JSON.stringify(queue.rounds[String(number)]) === sent) {
                delete queue.rounds[String(number)];
            }

            saveQueue();
            renderRounds();
        }

        if (queue.end) {
            const end = queue.end;
            const sent = JSON.stringify(end);

            if (!(await attempt(`/api/tournaments/${data.id}`, 'PATCH', end))) {
                return;
            }

            knownEnd = end;

            if (JSON.stringify(queue.end) === sent) {
                queue.end = null;
            }

            saveQueue();
        }
    }

    /**
     * One request: true when the app took it. A refusal keeps the draft and says why.
     * @param {string} url @param {string} method @param {unknown} body
     */
    async function attempt(url, method, body) {
        try {
            const response = await send(url, method, body);

            if (response.ok) {
                ui.problem.hidden = true;

                return true;
            }

            if (response.status === 401) {
                ui.login.hidden = false;

                return false;
            }

            const answer = await response.json().catch(() => null);
            ui.problem.textContent = (answer && answer.error) || `The app refused it (${response.status}).`;
            ui.problem.hidden = false;
        } catch {
            // no network: it waits, ⏳ turns ✓ by itself later
        }

        return false;
    }

    // ---------- sharing ----------

    /** @param {string} text */
    async function share(text) {
        ui.fallback.hidden = true;

        if (navigator.share) {
            try {
                await navigator.share({ text });

                return;
            } catch (error) {
                if (error && error.name === 'AbortError') {
                    return;
                }
            }
        }

        try {
            await navigator.clipboard.writeText(text);
            ui.sync.textContent = WORDS.copied;

            return;
        } catch {
            // no clipboard on plain HTTP: select the text for a manual copy
        }

        ui.fallback.value = text;
        ui.fallback.hidden = false;
        ui.fallback.focus();
        ui.fallback.select();
    }

    ui.form.addEventListener('submit', (event) => {
        event.preventDefault();
        const round = queueCurrent();

        if (!round) {
            return;
        }

        // the share sheet needs the tap's own moment: before any waiting
        const shared = share(line(round));
        void sync();
        void shared;
    });

    ui.save.addEventListener('click', () => {
        if (queueCurrent()) {
            void sync();
        }
    });

    ui.shareDay.addEventListener('click', () => {
        void share(dayText());
    });

    ui.retry.addEventListener('click', () => {
        void sync();
    });

    // ---------- the end of the day ----------

    ui.placement.value = knownEnd.placement ? String(knownEnd.placement) : '';
    ui.players.value = knownEnd.players ? String(knownEnd.players) : '';
    ui.conclusion.value = knownEnd.conclusion;

    if (queue.end) {
        ui.placement.value = queue.end.placement ? String(queue.end.placement) : '';
        ui.players.value = queue.end.players ? String(queue.end.players) : '';
        ui.conclusion.value = queue.end.conclusion;
    }

    ui.endForm.addEventListener('submit', (event) => {
        event.preventDefault();
        // 0 and "" empty a field in the app
        queue.end = {
            placement: Number(ui.placement.value) || 0,
            players: Number(ui.players.value) || 0,
            conclusion: ui.conclusion.value.trim(),
        };
        saveQueue();
        showSync();
        void sync();
    });

    // ---------- start ----------

    window.addEventListener('online', () => void sync());
    document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') {
            void sync();
        }
    });

    renderRounds();
    renderForm();
    showSync();
    void sync();
})();
