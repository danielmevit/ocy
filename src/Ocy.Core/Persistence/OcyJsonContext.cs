using System.Text.Json.Serialization;
using Ocy.Core.Engine;
using Ocy.Core.Settings;
using Ocy.Core.Statistics;

namespace Ocy.Core.Persistence;

[JsonSourceGenerationOptions(
    PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase,
    WriteIndented = true,
    UnmappedMemberHandling = JsonUnmappedMemberHandling.Skip)]
[JsonSerializable(typeof(OcySettings))]
[JsonSerializable(typeof(EngineState))]
[JsonSerializable(typeof(StatisticsData))]
[JsonSerializable(typeof(DailyStatistics))]
public partial class OcyJsonContext : JsonSerializerContext
{
}
