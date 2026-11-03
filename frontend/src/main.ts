import App from './App.svelte';
import './deck_builder.css';

const target = document.getElementById('app');

if (!target) throw new Error('the deck builder page has no #app element');

// the backend page passes the version id; `npm run dev` uses ?id=…
const deckVersionId = Number(
    target.dataset.deckVersionId || new URLSearchParams(location.search).get('id'),
);
target.replaceChildren();

new App({ target, props: { deckVersionId } });
