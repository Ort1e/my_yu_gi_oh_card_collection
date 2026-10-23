// The navigation bar of every page (templates/base.html, header.css).
//
// - The groups (Collection, Decks) are <details> menus: they open without
//   this script. It only closes the others when one opens, and closes them
//   on a click elsewhere or Escape.
// - Phones: the links hide behind the "Menu" button. Without this script the
//   button stays hidden and the links stay shown.
// - The link of the current page is marked (`aria-current`), and so is its
//   group's title.

(function () {
    const navbar = document.querySelector('.navbar');
    const links = document.getElementById('nav-links');
    const toggle = document.querySelector('.nav-toggle');

    if (!navbar || !links) {
        return;
    }

    const groups = [...navbar.querySelectorAll('.nav-group details')];

    function closeGroups(except) {
        for (const group of groups) {
            if (group !== except) {
                group.open = false;
            }
        }
    }

    for (const group of groups) {
        group.addEventListener('toggle', () => {
            if (group.open) {
                closeGroups(group);
            }
        });
    }

    document.addEventListener('click', (event) => {
        if (!navbar.contains(event.target)) {
            closeGroups(null);
        }
    });

    document.addEventListener('keydown', (event) => {
        if (event.key === 'Escape') {
            closeGroups(null);
        }
    });

    // Phones: the "Menu" button shows or hides the links.
    if (toggle) {
        navbar.classList.add('nav-collapsible');
        toggle.hidden = false;

        toggle.addEventListener('click', () => {
            const open = navbar.classList.toggle('nav-open');
            toggle.setAttribute('aria-expanded', String(open));
        });
    }

    // The current page: the link whose path is the longest start of the
    // page's path ("/decks/built/…" is "Built decks", not "Decks").
    const path = location.pathname;
    let current = null;
    let currentLength = 0;

    for (const link of links.querySelectorAll('a:not(.section-switch)')) {
        const linkPath = new URL(link.href).pathname;
        const matches = path === linkPath || path.startsWith(linkPath.replace(/\/?$/, '/'));

        if (matches && linkPath.length > currentLength) {
            current = link;
            currentLength = linkPath.length;
        }
    }

    if (current) {
        current.setAttribute('aria-current', 'page');

        const group = current.closest('.nav-group');

        if (group) {
            group.querySelector('summary').classList.add('nav-current');
        }
    }
})();
