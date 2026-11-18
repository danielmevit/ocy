namespace Ocy.Core.Engine;

public sealed record CurrentBreakInfo(
    BreakKind Kind,
    string ExerciseId,
    TimeSpan TotalDuration,
    TimeSpan RemainingDuration,
    IReadOnlyList<string>? SuggestionExerciseIds = null);
