// Filter modal of the list pages (templates/list_base.html): the badge opens
// it, a click on the backdrop closes it.

const badge = document.getElementById('filter-badge');
const modal = document.getElementById('filter-modal');

if (badge && modal) {
    badge.addEventListener('click', () => modal.classList.remove('hidden'));

    modal.addEventListener('click', (e) => {
        if (e.target === modal) modal.classList.add('hidden');
    });
}
