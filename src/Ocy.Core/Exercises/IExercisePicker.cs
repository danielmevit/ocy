namespace Ocy.Core.Exercises;

public interface IExercisePicker
{
    ExerciseDefinition PickShortExercise(bool hasWindowNearby, string? previousExerciseId = null);

    (ExerciseDefinition Body, ExerciseDefinition Eyes) PickLongExercises(
        bool hasWindowNearby,
        string? previousEyesId = null,
        string? previousBodyId = null);
}
