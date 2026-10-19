// Chart of /collection/value (templates/collection/value.html): the value of
// the copies owned at each price refresh, and what the lots cost by then
// (purchases − sales). The data is in `#value-chart-data`, built by
// routes/collection/mod.rs.

const chart = JSON.parse(document.getElementById('value-chart-data').textContent);

new Chart(document.getElementById('value_chart'), {
    type: 'line',
    data: {
        labels: chart.labels,
        datasets: [
            {
                label: 'Value (€)',
                data: chart.value,
                borderColor: 'rgba(4, 146, 111, 1)',
                backgroundColor: 'rgba(4, 146, 111, 0.2)',
                borderWidth: 2,
                tension: 0.3,
                fill: true,
            },
            {
                label: 'Spent − sold (€)',
                data: chart.net_spent,
                borderColor: 'rgba(0, 24, 163, 1)',
                borderWidth: 2,
                borderDash: [6, 4],
                tension: 0.3,
                fill: false,
            },
        ],
    },
    options: {
        responsive: true,
        interaction: { mode: 'index', intersect: false },
        scales: {
            y: { beginAtZero: true, title: { display: true, text: '€' } },
        },
    },
});
