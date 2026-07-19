import Foundation

/// Small, allocation-light formatters shared across the UI. Kept as a namespace so views never
/// spin up a `DateFormatter` in a hot path.
enum Formatting {
    private static let clockFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "h:mm a"
        return formatter
    }()

    private static let dayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .none
        return formatter
    }()

    private static let exportStampFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH.mm.ss"
        return formatter
    }()

    /// A monospaced `m:ss` (or `h:mm:ss`) timer string, used for both the live timer and durations.
    static func timer(_ interval: TimeInterval) -> String {
        let clamped = max(0, interval)
        let totalSeconds = Int(clamped.rounded(.down))
        let hours = totalSeconds / 3_600
        let minutes = (totalSeconds % 3_600) / 60
        let seconds = totalSeconds % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        }
        return String(format: "%d:%02d", minutes, seconds)
    }

    /// A spoken duration used for accessibility values, e.g. "1 minute, 5 seconds".
    static func spokenDuration(_ interval: TimeInterval) -> String {
        let totalSeconds = Int(max(0, interval).rounded())
        let minutes = totalSeconds / 60
        let seconds = totalSeconds % 60
        var parts: [String] = []
        if minutes > 0 {
            parts.append("\(minutes) minute\(minutes == 1 ? "" : "s")")
        }
        parts.append("\(seconds) second\(seconds == 1 ? "" : "s")")
        return parts.joined(separator: ", ")
    }

    /// A relative day plus clock time, e.g. "Today at 3:41 PM" / "Jul 18 at 9:02 AM".
    static func timestamp(_ date: Date, relativeTo now: Date = Date()) -> String {
        let calendar = Calendar.current
        let time = clockFormatter.string(from: date)
        if calendar.isDateInToday(date) {
            return "Today at \(time)"
        }
        if calendar.isDateInYesterday(date) {
            return "Yesterday at \(time)"
        }
        return "\(dayFormatter.string(from: date)) at \(time)"
    }

    static func exportStamp(_ date: Date) -> String {
        exportStampFormatter.string(from: date)
    }

    /// A time-of-day greeting for the Today header.
    static func greeting(for date: Date = Date()) -> String {
        switch Calendar.current.component(.hour, from: date) {
        case 5..<12: "Good morning"
        case 12..<17: "Good afternoon"
        case 17..<22: "Good evening"
        default: "Good evening"
        }
    }
}

/// A single source of truth for how an untitled capture reads in lists and the reader.
enum SessionDisplay {
    static let untitled = "Untitled capture"

    static func title(_ rawTitle: String) -> String {
        let trimmed = rawTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? untitled : trimmed
    }
}
