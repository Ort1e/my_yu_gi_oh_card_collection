from django.db import models

CALENDAR_PHOTO_FOLDER = 'advent_calendar_photos/'


class AdventCalendar(models.Model):
    year = models.PositiveIntegerField(unique=True, help_text="The year of the Advent Calendar")

    def __str__(self):
        return f"Advent Calendar {self.year}"
    


class AdventCalendarEntry(models.Model):
    calendar = models.ForeignKey(
        AdventCalendar,
        related_name="entries",
        on_delete=models.CASCADE
    )
    day = models.PositiveIntegerField(help_text="The day of the month (1-25)")
    card = models.ForeignKey(
        'Card',
        related_name="advent_calendar_entries",
        on_delete=models.PROTECT
    )
    photo = models.ImageField(upload_to=CALENDAR_PHOTO_FOLDER, null=True, blank=True)

    class Meta:
        unique_together = ('calendar', 'day')

    def __str__(self):
        return f"Day {self.day} of {self.calendar}: {self.card}"
    
    def date(self):
        from datetime import date
        return date(self.calendar.year, 12, self.day)