namespace Ocy.Core.Exercises;

public sealed record ExerciseDefinition(
    string Id,
    ExerciseKind Kind,
    bool RequiresWindow,
    TimeSpan SuggestedDuration)
{
    public string TitleKey => $"exercise.{Id}.title";

    public string BodyKey => $"exercise.{Id}.body";
}
