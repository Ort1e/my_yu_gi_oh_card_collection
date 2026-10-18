// The edit page of an advent calendar (templates/advent/edit.html): ask
// before emptying a day.

document.addEventListener('submit', (e) => {
    const message = e.target.dataset.confirm;

    if (message && !confirm(message)) e.preventDefault();
});
