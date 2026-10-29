const absolute = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const units = [
  ["year", 365 * 24 * 60 * 60],
  ["month", 30 * 24 * 60 * 60],
  ["week", 7 * 24 * 60 * 60],
  ["day", 24 * 60 * 60],
  ["hour", 60 * 60],
  ["minute", 60],
  ["second", 1],
];

function relativeText(seconds, precision) {
  if (precision === "hour" && seconds <= 0 && seconds > -60 * 60) {
    return "within the last hour";
  }
  const smallest = precision === "hour" ? "hour" : "second";
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size || unit === smallest) {
      return relative.format(Math.round(seconds / size), unit);
    }
  }
}

function localizeTimes() {
  for (const time of document.querySelectorAll("time[datetime]")) {
    const date = new Date(time.dateTime);
    const exact = absolute.format(date);
    if (time.dataset.format === "relative") {
      time.textContent = relativeText((date.getTime() - Date.now()) / 1000, time.dataset.precision);
      time.title = exact;
    } else {
      time.textContent = exact;
    }
  }
}

localizeTimes();
document.addEventListener("htmx:afterSettle", localizeTimes);
setInterval(localizeTimes, 60 * 1000);
