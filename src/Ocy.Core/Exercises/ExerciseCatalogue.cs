namespace Ocy.Core.Exercises;

public static class ExerciseCatalogue
{
    public static readonly IReadOnlyList<ExerciseDefinition> All = new List<ExerciseDefinition>
    {
        // Eyes (9 exercises)
        new("blink", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(15)),
        new("close-tight", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(15)),
        new("roll", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(15)),
        new("look-up-down", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(15)),
        new("look-left-right", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(15)),
        new("near-far", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(15)),
        new("window-far", ExerciseKind.Eyes, RequiresWindow: true, TimeSpan.FromSeconds(20)),
        new("palming", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(20)),
        new("figure-eight", ExerciseKind.Eyes, RequiresWindow: false, TimeSpan.FromSeconds(20)),

        // Body (3 exercises)
        new("neck-shoulders", ExerciseKind.Body, RequiresWindow: false, TimeSpan.FromSeconds(60)),
        new("stand-walk", ExerciseKind.Body, RequiresWindow: false, TimeSpan.FromSeconds(120)),
        new("stretch-arms", ExerciseKind.Body, RequiresWindow: false, TimeSpan.FromSeconds(60)),
    }.AsReadOnly();

    public static readonly IReadOnlyDictionary<string, ExerciseDefinition> ById =
        All.ToDictionary(e => e.Id, StringComparer.OrdinalIgnoreCase);
}
