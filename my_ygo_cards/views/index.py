from collections import defaultdict
from decimal import Decimal
from django.shortcuts import render
from my_ygo_cards.models import Lot, MonthlyBudget, Unite
from my_ygo_cards.views.budget import BudgetTotals, RecapEntry

TOTAL_MONTHS_TO_DISPLAY = 5


def index(request):
    """
    Index view that renders the home page with monthly spending chart.
    """
    recap_entries = RecapEntry.from_db(prev=True)
    
    # Chart labels (shared)
    sorted_recap = sorted(recap_entries, key=lambda entry: entry.month)
    sorted_months = [entry.month for entry in sorted_recap]
    labels = [entry.month_str for entry in sorted_recap]

    # Bar chart datasets
    card_spending = [float(entry.spent - entry.shipping_cost) for entry in reversed(recap_entries)]
    shipping_costs = [float(entry.shipping_cost) for entry in reversed(recap_entries)]

    # Card count chart
    lots = Lot.objects.filter(is_cancelled=False, lot_type=Lot.PURCHASE).prefetch_related("unite_set")
    card_count_by_month = defaultdict(int)
    
    for lot in lots:
        month_key = lot.buy_date.replace(day=1)
        card_count_by_month[month_key] += lot.cards.count()
    
    card_counts = [card_count_by_month[m] for m in sorted_months]

    # card costs
    unites = Unite.objects.filter(lot__in=lots)
    number_by_card_cost = defaultdict(int)
    for unite in unites:
        if unite.price is not None:
            number_by_card_cost[float(unite.price)] += 1

    ordonned_cards_costs = sorted(number_by_card_cost.keys())

    

    # Budget overview

    recaps_to_display = sorted_recap[-TOTAL_MONTHS_TO_DISPLAY:] if len(sorted_recap) > TOTAL_MONTHS_TO_DISPLAY else sorted_recap

    totals = BudgetTotals.compute(recap_entries)

    last_budget = MonthlyBudget.objects.last()
    last_budget_amount = last_budget.amount if last_budget else Decimal('0.0')
    available_debt = last_budget_amount * 3

    return render(request, "my_ygo_cards/index.html", {
        # chart
        "labels": labels,
        "card_spending": card_spending,
        "shipping_costs": shipping_costs,
        "card_counts": card_counts,
        "ordonned_cards_costs": ordonned_cards_costs,
        "number_by_card_cost": dict(number_by_card_cost),
        # recap
        "recap_entries": recaps_to_display,
        "totals": totals,
        'available_debt': available_debt,
    })
