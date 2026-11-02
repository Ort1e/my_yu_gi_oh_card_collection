// The "Read" switches of the book tables and pages
// (`<input type="checkbox" data-book-read="<book id>">`): a change is saved
// at once (POST /books/api/books/:id/read). When it can't be (not logged in,
// a user who may not change the books), the switch goes back and says why.

const reasons = {
    401: 'Log in to change it.',
    403: 'You may not change the books.',
    404: 'This book no longer exists.',
};

async function save(input) {
    const read = input.checked;
    input.disabled = true;

    try {
        const response = await fetch(`/books/api/books/${input.dataset.bookRead}/read`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ read }),
        });

        if (!response.ok) {
            throw new Error(reasons[response.status] ?? `Not saved (HTTP ${response.status}).`);
        }

        // the table sorts by the new value
        const cell = input.closest('td');
        if (cell) cell.dataset.sortValue = read ? '1' : '0';
    } catch (error) {
        input.checked = !read;
        alert(error.message);
    } finally {
        input.disabled = false;
    }
}

// one listener for the page: the random pick's switch gets its book later
document.addEventListener('change', (event) => {
    if (event.target.matches('input[data-book-read]')) save(event.target);
});
