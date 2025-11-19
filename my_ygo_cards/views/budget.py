from dataclasses import dataclass
from decimal import Decimal
from datetime import date
from typing import List, Dict
from collections import defaultdict
import calendar

from django.db.models.manager import BaseManager
from django.shortcuts import render
from django.http import HttpRequest, HttpResponse
from django.db.models import Sum
from ..models import Lot, MonthlyBudget


def get_month_range(start: date, end: date) -> List[date]:
    """Return a list of the first day of each month between start and end (inclusive)."""
    months: List[date] = []
    current = start.replace(day=1)
    end = end.replace(day=1)
    while current <= end:
        months.append(current)
        if current.month == 12:
            current = current.replace(year=current.year + 1, month=1)
        else:
            current = current.replace(month=current.month + 1)
    return months


def get_first_month(lots : BaseManager[Lot], budgets : BaseManager[MonthlyBudget]) -> date:
    """Determine the first month to start reporting from."""
    if budgets.exists():
        budget = budgets.first()
        if budget is not None:
            return budget.month.replace(day=1)
    elif lots.exists():
        lot = lots.order_by('buy_date').first()
        if lot is not None:
            return lot.buy_date.replace(day=1)
    else:
        return date.today().replace(day=1)
    
    return date.today().replace(day=1)


def build_budget_map(budgets: List[MonthlyBudget], months: List[date]) -> Dict[date, Decimal]:
    """Return a dict of month -> budget (carried forward) including ponctual apports."""
    budget_map: Dict[date, Decimal] = {}
    last_budget: Decimal = Decimal('0.0')
    budget_idx: int = 0

    budgets_list = list(budgets)

    for month in months:
        # update base recurring budget when a new MonthlyBudget is encountered
        while budget_idx < len(budgets_list) and budgets_list[budget_idx].month <= month:
            last_budget = budgets_list[budget_idx].amount
            budget_idx += 1

        # apports for this month
        apports_sum = (
            MonthlyBudget.objects.filter(month=month)
            .aggregate(total=Sum("apports__amount"))["total"] or Decimal("0.0")
        )

        # sold lots for this month
        sold_lots_sum = (
            Lot.objects.filter(buy_date__year=month.year, buy_date__month=month.month, lot_type=Lot.SALE)
            .aggregate(total=Sum("price"))["total"] or Decimal("0.0")
        )


        budget_map[month] = last_budget + apports_sum + sold_lots_sum

    return budget_map


def build_monthly_spending(lots: List[Lot]) -> Dict[date, Dict[str, Decimal]]:
    """Aggregate lots into a dict of month -> {'spent': total, 'budget': placeholder}."""
    spending: Dict[date, Dict[str, Decimal]] = defaultdict(lambda: {
        'spent': Decimal('0.0'),
        'budget': Decimal('0.0'),
        'shpping_cost': Decimal('0.0'),
    })
    for lot in lots:
        month = lot.buy_date.replace(day=1)
        spending[month]['spent'] += lot.price
        spending[month]['shpping_cost'] += lot.shipping_cost
    return spending

def add_one_month(d: date) -> date:
    """Return the first day of the next month."""
    if d.month == 12:
        return date(d.year + 1, 1, d.day)
    return date(d.year, d.month + 1, d.day)

@dataclass
class RecapEntry:
    month: date
    spent: Decimal
    budget: Decimal
    debt: Decimal
    shipping_cost: Decimal
    shipping_percent: Decimal = Decimal('0.0')
    selling_revenue: Decimal = Decimal('0.0')
    selling_shipping_cost: Decimal = Decimal('0.0')
    selling_real_revenue: Decimal = Decimal('0.0')

    def __init__(self, month: date, spent: Decimal, budget: Decimal, shipping_cost: Decimal, selling_revenue: Decimal, selling_shipping_cost):
        self.month = month
        self.spent = spent
        self.budget = budget
        self.shipping_cost = shipping_cost
        self.debt = spent - budget
        self.shipping_percent = (shipping_cost / spent * 100) if spent > 0 else Decimal('0.0')
        self.selling_revenue = selling_revenue
        self.selling_shipping_cost = selling_shipping_cost
        self.selling_real_revenue = selling_revenue - selling_shipping_cost

    @classmethod
    def from_db(cls, prev : bool = False) -> List['RecapEntry']:
        """
        Build recap entries by querying the database internally.
        If prev is True, include the current month in the recap.
        """
        purchased_lots: BaseManager[Lot] = Lot.objects.filter(is_cancelled=False, lot_type=Lot.PURCHASE)
        sold_lots: BaseManager[Lot] = Lot.objects.filter(is_cancelled=False, lot_type=Lot.SALE)
        budgets: BaseManager[MonthlyBudget] = MonthlyBudget.objects.order_by('month')

        first_month: date = get_first_month(purchased_lots, budgets)
        last_month: date 
        if prev:
            last_month = add_one_month(date.today().replace(day=1))
        else:
            last_month = date.today().replace(day=1)
        months: List[date] = get_month_range(first_month, last_month)

        monthly_spending = build_monthly_spending(list(purchased_lots))
        budget_map = build_budget_map(list(budgets), months)

        monthly_sold_revenue = build_monthly_spending(list(sold_lots))

        entries: List[RecapEntry] = []
        for month in sorted(months, reverse=True):
            spent = monthly_spending[month]['spent']
            shipping_cost = monthly_spending[month]['shpping_cost']
            selling_revenue = monthly_sold_revenue[month]['spent']
            budget = budget_map.get(month, Decimal('0.0'))
            selling_shipping_cost = monthly_sold_revenue[month]['shpping_cost']
            entries.append(cls(month=month, spent=spent, budget=budget, shipping_cost=shipping_cost, selling_revenue=selling_revenue, selling_shipping_cost=selling_shipping_cost))

        return entries
    
    @property
    def month_str(self) -> str:
        """Return the month as a formatted string."""

        return f"{calendar.month_name[self.month.month]} {self.month.year}"

@dataclass
class BudgetTotals:
    total_budget: Decimal
    total_spent: Decimal
    total_debt: Decimal
    total_shipping_cost: Decimal
    total_shipping_percent: Decimal
    total_selling_revenue: Decimal
    total_selling_shipping_cost: Decimal
    total_real_selling_revenue: Decimal

    @staticmethod
    def compute(recap_entries: List['RecapEntry']) -> "BudgetTotals":
        """Compute total budget, spent, debt, shipping cost, and shipping percent from recap entries."""
        total_budget: Decimal = sum((entry.budget for entry in recap_entries), Decimal('0.0'))
        total_spent: Decimal = sum((entry.spent for entry in recap_entries), Decimal('0.0'))
        total_debt: Decimal = sum((entry.debt for entry in recap_entries), Decimal('0.0'))
        total_shipping_cost: Decimal = sum((entry.shipping_cost for entry in recap_entries), Decimal('0.0'))
        total_shipping_percent: Decimal = (
            (total_shipping_cost / total_spent * 100) if total_spent > 0 else Decimal('0.0')
        )
        total_selling_revenue: Decimal = sum((entry.selling_revenue for entry in recap_entries), Decimal('0.0'))
        total_selling_shipping_cost: Decimal = sum((entry.selling_shipping_cost for entry in recap_entries), Decimal('0.0'))
        total_real_selling_revenue = total_selling_revenue - total_selling_shipping_cost

        return BudgetTotals(
            total_budget=total_budget,
            total_spent=total_spent,
            total_debt=total_debt,
            total_shipping_cost=total_shipping_cost,
            total_shipping_percent=total_shipping_percent,
            total_selling_revenue=total_selling_revenue,
            total_selling_shipping_cost=total_selling_shipping_cost,
            total_real_selling_revenue=total_real_selling_revenue,
        )


def budget_overview(request: HttpRequest) -> HttpResponse:
    prev_recap_entries = RecapEntry.from_db(prev=True)
    totals_prev = BudgetTotals.compute(prev_recap_entries)

    recap_entries = RecapEntry.from_db(prev=False)
    totals = BudgetTotals.compute(recap_entries)

    last_budget = MonthlyBudget.objects.last()
    last_budget_amount = last_budget.amount if last_budget else Decimal('0.0')
    available_debt = last_budget_amount * 3

    return render(request, 'my_ygo_cards/budget/overview.html', {
        'recap': prev_recap_entries,
        'totals_prev': totals_prev,
        "totals": totals,
        'available_debt': available_debt,
    })