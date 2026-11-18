namespace Ocy.Core.Engine;

public sealed record BreakInfo(
    BreakKind Kind,
    string ExerciseId,
    TimeSpan TotalDuration,
    IReadOnlyList<string>? SuggestionExerciseIds = null);
