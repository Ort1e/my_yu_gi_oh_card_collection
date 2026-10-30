// Every page (templates/base.html): a form with `data-confirm` asks first,
// with that message (a deletion). An `onclick="return confirm(…)"` in the
// page would not run: the Content-Security-Policy allows no script written
// in the page.

document.addEventListener('submit', (event) => {
    const message = event.target.dataset.confirm;

    if (message && !confirm(message)) event.preventDefault();
});
