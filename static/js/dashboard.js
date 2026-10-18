// Charts of the dashboard (templates/dashboard.html). The data is in the
// `#charts-data` JSON block, built by routes/dashboard.rs.

const charts = JSON.parse(document.getElementById('charts-data').textContent);

new Chart(document.getElementById('combined_chart'), {
    data: {
        labels: charts.labels,
        datasets: [
            {
                type: 'bar',
                label: 'Shipping Cost (€)',
                data: charts.shipping_costs,
                backgroundColor: 'rgba(255, 206, 86, 0.5)',
                borderColor: 'rgba(255, 206, 86, 1)',
                borderWidth: 1,
                stack: 'spending',
                yAxisID: 'y',
            },
            {
                type: 'bar',
                label: 'Card Spending (€)',
                data: charts.card_spending,
                backgroundColor: 'rgba(54, 162, 235, 0.5)',
                borderColor: 'rgba(54, 162, 235, 1)',
                borderWidth: 1,
                stack: 'spending',
                yAxisID: 'y',
            },
            {
                type: 'line',
                label: 'Cards Added Per Month',
                data: charts.card_counts,
                backgroundColor: 'rgba(255, 99, 132, 0.3)',
                borderColor: 'rgba(255, 99, 132, 1)',
                borderWidth: 2,
                tension: 0.3,
                fill: true,
                yAxisID: 'y1',
            },
        ],
    },
    options: {
        responsive: true,
        interaction: { mode: 'index', intersect: false },
        scales: {
            x: { stacked: true },
            y: {
                type: 'linear',
                position: 'left',
                beginAtZero: true,
                stacked: true,
                title: { display: true, text: '€' },
            },
            y1: {
                type: 'linear',
                position: 'right',
                beginAtZero: true,
                grid: { drawOnChartArea: false },
                title: { display: true, text: 'Cards Added' },
            },
        },
    },
});

new Chart(document.getElementById('card_cost_chart'), {
    type: 'line',
    data: {
        datasets: [
            {
                label: 'Number of Cards by Cost',
                data: charts.cards_by_cost,
                backgroundColor: 'rgba(75, 192, 192, 0.3)',
                borderColor: 'rgba(75, 192, 192, 1)',
                borderWidth: 2,
                tension: 0.3,
                fill: true,
            },
        ],
    },
    options: {
        responsive: true,
        scales: {
            x: { type: 'logarithmic', title: { display: true, text: 'Cost (€)' } },
            y: { beginAtZero: true, title: { display: true, text: 'Number of Cards' } },
        },
    },
});
