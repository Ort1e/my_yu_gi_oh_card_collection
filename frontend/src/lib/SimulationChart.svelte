<script>
    // Result of the Monte Carlo simulation as one bar per category, split by
    // how many cards of it the opening hand has (0, 1, 2, 3+). The share of
    // hands with at least one card is written at the right of each bar.
    import { onDestroy } from 'svelte';
    import { Chart, registerables } from 'chart.js';

    Chart.register(...registerables);

    export let result;

    // One blue from light to dark: more cards, darker (an ordered scale, so a
    // single hue; checked for contrast and colour-blind readability).
    const SEGMENTS = [
        { label: '0 cards', color: '#86b6ef', min: 0, max: 0 },
        { label: '1 card', color: '#3987e5', min: 1, max: 1 },
        { label: '2 cards', color: '#1c5cab', min: 2, max: 2 },
        { label: '3+ cards', color: '#0d366b', min: 3, max: Infinity },
    ];

    const INK = '#1a1a1a';
    const MUTED = '#5c5c5c';
    const GRID = '#e6e6e6';

    let canvas;
    let chart;

    /** % of hands whose count of the category is between min and max */
    function share(stats, { min, max }) {
        let hands = 0;

        for (const [n, count] of Object.entries(stats.per_hand)) {
            if (Number(n) >= min && Number(n) <= max) hands += count;
        }

        return (hands / result.total_simulations) * 100;
    }

    // writes "≥1: 77%" at the right of each bar
    const atLeastOneLabels = {
        id: 'atLeastOneLabels',

        afterDatasetsDraw(chart) {
            const { ctx, chartArea } = chart;
            const zero = chart.data.datasets[0];

            ctx.save();
            ctx.fillStyle = INK;
            ctx.font = '600 12px sans-serif';
            ctx.textBaseline = 'middle';

            chart.getDatasetMeta(0).data.forEach((bar, i) => {
                ctx.fillText(`≥1: ${(100 - zero.data[i]).toFixed(0)}%`, chartArea.right + 8, bar.y);
            });

            ctx.restore();
        },
    };

    function draw(result) {
        chart?.destroy();

        const stats = result.categories;

        chart = new Chart(canvas, {
            type: 'bar',
            data: {
                labels: stats.map((s) => s.name),
                datasets: SEGMENTS.map((segment) => ({
                    label: segment.label,
                    data: stats.map((s) => share(s, segment)),
                    backgroundColor: segment.color,
                    // a thin white line between segments, not a border around them
                    borderColor: '#fff',
                    borderWidth: { right: 2 },
                    borderSkipped: false,
                    barThickness: 20,
                })),
            },
            options: {
                indexAxis: 'y',
                maintainAspectRatio: false,
                animation: false,
                layout: { padding: { right: 70 } },
                color: INK,
                plugins: {
                    title: {
                        display: true,
                        text: `Cards of each category in the opening hand (${result.total_simulations} hands)`,
                        color: INK,
                        font: { size: 14, weight: '600' },
                    },
                    legend: {
                        position: 'top',
                        align: 'start',
                        labels: { color: INK, boxWidth: 14, boxHeight: 14 },
                    },
                    tooltip: {
                        callbacks: {
                            label: (c) => `${c.dataset.label}: ${c.parsed.x.toFixed(1)}% of hands`,
                        },
                    },
                },
                scales: {
                    x: {
                        stacked: true,
                        min: 0,
                        max: 100,
                        ticks: { color: MUTED, callback: (v) => `${v}%` },
                        grid: { color: GRID },
                    },
                    y: {
                        stacked: true,
                        ticks: { color: INK },
                        grid: { display: false },
                    },
                },
            },
            plugins: [atLeastOneLabels],
        });
    }

    $: if (canvas && result) draw(result);

    onDestroy(() => chart?.destroy());
</script>

<div class="sim-chart" style="height: {90 + result.categories.length * 30}px">
    <canvas bind:this={canvas}></canvas>
</div>

<style>
    /* The simulation chart, on a white panel so its text is readable. */
    .sim-chart {
        margin: 1rem 0;
        padding: 0.75rem;
        border-radius: 8px;
        background: #fff;
    }
</style>
