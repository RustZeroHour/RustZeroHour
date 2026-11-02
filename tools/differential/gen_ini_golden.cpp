// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of RustZeroHour, a derivative of the Zero Hour source
// release. Copyright 2025 Electronic Arts Inc.; Rust port Copyright 2026 the
// RustZeroHour contributors.
//
// rzh-ini golden-vector generator (differential harness).
//
// Verbatim transcription of the INI engine at baseline commit 571a7c5
// (Zero Hour configuration, release-build branches: DEBUG_ASSERTCRASH and
// DEBUG_CRASH are no-ops) plus the three task-10 validation block types:
//
//   Core/GameEngine/Include/Common/INI.h
//   Core/GameEngine/Source/Common/INI/INI.cpp   (reader, strtok tokenizing,
//     scanType<T> with USE_STD_FROM_CHARS_PARSING = 1, initFromINI,
//     the bit-string and quoted-string readers, the unit conversions)
//   Core/GameEngine/Include/Common/GameCommon.h (frame conversion constants)
//   GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Armor.cpp (+ the
//     damage-type name list from Core/.../Damage.cpp)
//   GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Locomotor.cpp
//     (+ the name lists from Locomotor.h / LocomotorSet.h; the disabled
//     PRESERVE_RETAIL_PHYSICS_FORWARD_SPEED_AVERAGE machinery is omitted
//     exactly as in the baseline default build)
//   GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Weapon.cpp (the
//     ported field subset, parseShotDelay, the store semantics including
//     the unreachable-override quirk)
//
// Harness instrumentation (documented deviations from the letter of the
// original, needed to make failures dumpable): the throw sites raise a
// structured IniError {kind, file, line} instead of the flat INI_* codes
// whose values are all ERROR_BAD_INI, and INI::load's catch-rethrow keeps
// the structured payload instead of formatting a message string.
//
// The stores key templates by name (std::map<std::string, ...>): the Rust
// port keys by name-key ordinal, and because a block name is interned in
// the same parse step that inserts the template, ordinal order equals
// first-seen order equals this vector order, so the two dumps iterate the
// same sequence.
//
// Usage: gen_ini_golden <output-file> [fixtures-dir]

#include <charconv>
#include <climits>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <string_view>
#include <type_traits>
#include <vector>
#include <cmath>

#if !defined(_WIN32)
#include <strings.h>
#define stricmp strcasecmp
#define strnicmp strncasecmp
#endif

// ----------------------------------------------------------------------------
// engine
// ----------------------------------------------------------------------------

typedef int Int;
typedef unsigned UnsignedInt;
typedef unsigned char Byte;
typedef float Real;
typedef bool Bool;

enum LoadType
{
	INI_LOAD_OVERWRITE,
	INI_LOAD_CREATE_OVERRIDES,
	INI_LOAD_MULTIFILE
};

static const char* loadTypeName(LoadType t)
{
	switch (t)
	{
		case INI_LOAD_OVERWRITE: return "OVERWRITE";
		case INI_LOAD_CREATE_OVERRIDES: return "CREATE_OVERRIDES";
		case INI_LOAD_MULTIFILE: return "MULTIFILE";
	}
	return "?";
}

enum
{
	INI_MAX_CHARS_PER_LINE = 1028
};

struct IniError
{
	const char* kind;
	std::string file;
	unsigned line;
};

static const char* const SEPS = " \n\r\t=";
static const char* const SEPS_PERCENT = " \n\r\t=%";
static const char* const SEPS_COLON = " \n\r\t=:";
static const char* const SEPS_QUOTE = "\"\n=";

// BSD strlcpy/strlcat, absent from glibc.
static size_t strlcpy_local(char* dst, const char* src, size_t siz)
{
	const char* s = src;
	size_t n = siz;
	while (n > 1 && *s)
	{
		*dst++ = *s++;
		--n;
	}
	if (n > 0)
		*dst = 0;
	return strlen(src);
}

static size_t strlcat_local(char* dst, const char* src, size_t siz)
{
	size_t dlen = strnlen(dst, siz);
	if (dlen == siz)
		return dlen + strlen(src);
	return dlen + strlcpy_local(dst + dlen, src, siz - dlen);
}

// WWMath::PI (wwmath.h) and the GameCommon.h frame constants
// (WWSyncPerSecond = 30).
static const Real PI = 3.14159265358979323846f;
static const Real LOGICFRAMES_PER_MSEC_REAL = (((Real)30) / ((Real)1000));
static const Real SECONDS_PER_LOGICFRAME_REAL = 1.0f / (Real)30;

static Real ConvertDurationFromMsecsToFrames(Real msec) { return msec * LOGICFRAMES_PER_MSEC_REAL; }
static Real ConvertVelocityInSecsToFrames(Real distPerSec) { return distPerSec * SECONDS_PER_LOGICFRAME_REAL; }
static Real ConvertAccelerationInSecsToFrames(Real distPerSec2)
{
	const Real SEC_PER_LOGICFRAME_SQR = SECONDS_PER_LOGICFRAME_REAL * SECONDS_PER_LOGICFRAME_REAL;
	return distPerSec2 * SEC_PER_LOGICFRAME_SQR;
}
static Real ConvertAngularVelocityInDegreesPerSecToRadsPerFrame(Real degPerSec)
{
	const Real RADS_PER_DEGREE = PI / 180.0f;
	return degPerSec * (SECONDS_PER_LOGICFRAME_REAL * RADS_PER_DEGREE);
}
static Real Ceilf(Real x) { return ceilf(x); }

// INI::scanType<T> — verbatim from INI.cpp (USE_STD_FROM_CHARS_PARSING = 1).
template <typename Type>
Type scanType(const char* tokenString, const char* file, unsigned line)
{
	std::string_view token(tokenString);

	const size_t tokenOffset = (token[0] == '+');
	if (tokenOffset && token.size() > 1 && token[1] == '-')
		throw IniError{"INI_INVALID_DATA", file, line};

	std::conditional_t<std::is_integral_v<Type>, int64_t, Type> result{};
	const auto [ptr, ec] = std::from_chars(token.data() + tokenOffset, token.data() + token.size(), result);

	if (ec == std::errc::result_out_of_range)
	{
		char* end;
		Type value;
		if constexpr (std::is_floating_point_v<Type>)
			value = std::strtof(tokenString, &end);
		else if constexpr (std::is_signed_v<Type>)
			value = static_cast<Type>(std::strtoll(tokenString, &end, 10));
		else
			value = static_cast<Type>(std::strtoull(tokenString, &end, 10));

		if (end == tokenString)
			throw IniError{"INI_INVALID_DATA", file, line};

		return value;
	}

	if (ec != std::errc{})
	{
		throw IniError{"INI_INVALID_DATA", file, line};
	}

	return static_cast<Type>(result);
}

static Int scanInt(const char* token, const char* file, unsigned line)
{
	return scanType<Int>(token, file, line);
}

static UnsignedInt scanUnsignedInt(const char* token, const char* file, unsigned line)
{
	return scanType<UnsignedInt>(token, file, line);
}

static Real scanReal(const char* token, const char* file, unsigned line)
{
	return scanType<Real>(token, file, line);
}

static Real scanPercentToReal(const char* token, const char* file, unsigned line)
{
	return scanReal(token, file, line) / 100.0f;
}

static Bool scanBool(const char* token, const char* file, unsigned line)
{
	if (stricmp(token, "yes") == 0)
		return true;
	else if (stricmp(token, "no") == 0)
		return false;
	throw IniError{"INI_INVALID_DATA", file, line};
}

static Int scanIndexList(const char* token, const char* const* nameList, const char* file, unsigned line)
{
	Int count = 0;
	for (const char* const* name = nameList; *name; name++, count++)
	{
		if (stricmp(*name, token) == 0)
			return count;
	}
	throw IniError{"INI_INVALID_DATA", file, line};
}

class INI;

typedef void (*INIFieldParseProc)(INI* ini, void* instance, void* store, const void* userData);

struct FieldParse
{
	const char* token;
	INIFieldParseProc parse;
	const void* userData;
	int offset;
};

// ----------------------------------------------------------------------------
// the reader
// ----------------------------------------------------------------------------

class INI
{
public:
	std::vector<uint8_t> m_data;
	size_t m_readBufferNext = 0;
	std::string m_filename = "None";
	LoadType m_loadType = INI_LOAD_OVERWRITE;
	unsigned m_lineNum = 0;
	char m_buffer[INI_MAX_CHARS_PER_LINE + 1] = {0};
	size_t m_cend = 0;
	bool m_endOfFile = false;

	const char* getFilename() const { return m_filename.c_str(); }
	LoadType getLoadType() const { return m_loadType; }
	unsigned getLineNum() const { return m_lineNum; }
	bool isEOF() const { return m_endOfFile; }
	static const char* getSeps() { return SEPS; }
	static const char* getSepsPercent() { return SEPS_PERCENT; }
	static const char* getSepsColon() { return SEPS_COLON; }
	static const char* getSepsQuote() { return SEPS_QUOTE; }
	static const char* getEndToken() { return "End"; }

	[[noreturn]] void fail(const char* kind) const { throw IniError{kind, m_filename, m_lineNum}; }

	// INI::readLine — verbatim byte-level semantics.
	void readLine()
	{
		if (m_endOfFile)
		{
			*m_buffer = 0;
			m_cend = 0;
		}
		else
		{
			char* p = m_buffer;
			while (p != m_buffer + INI_MAX_CHARS_PER_LINE)
			{
				if (m_readBufferNext == m_data.size())
				{
					m_endOfFile = true;
					*p = 0;
					break;
				}
				*p = (char)m_data[m_readBufferNext++];
				if (*p == '\n')
				{
					*p = 0;
					break;
				}
				if (*p == ';')
				{
					*p = 0;
				}
				else if (*p > 0 && *p < 32)
				{
					*p = ' ';
				}
				p++;
			}
			*p = 0;
			m_cend = strlen(m_buffer);
			m_lineNum++;
		}
	}

	const char* firstToken(const char* seps) { return strtok(m_buffer, seps); }
	const char* nextToken(const char* seps) { return strtok(nullptr, seps); }

	const char* getNextToken(const char* seps = getSeps())
	{
		const char* token = ::strtok(nullptr, seps);
		if (!token)
			fail("INI_INVALID_DATA");
		return token;
	}

	const char* getNextTokenOrNull(const char* seps = getSeps()) { return ::strtok(nullptr, seps); }

	const char* getNextSubToken(const char* expected)
	{
		const char* token = getNextToken(getSepsColon());
		if (stricmp(token, expected) != 0)
			fail("INI_INVALID_DATA");
		return getNextToken(getSepsColon());
	}

	// INI::getNextQuotedAsciiString — verbatim (the buff[strLen-2] probe is
	// bounds-guarded here; the original reads out of bounds for
	// >1027-char quoted tokens, which the fixtures never contain).
	std::string getNextQuotedAsciiString()
	{
		char buff[INI_MAX_CHARS_PER_LINE];
		buff[0] = '\0';
		const char* token = getNextTokenOrNull();
		if (token != nullptr)
		{
			if (token[0] != '"')
			{
				return token;
			}
			else
			{
				int strLen = 0;
				Bool done = false;
				if ((strLen = (int)strlen(token)) > 1)
				{
					strlcpy_local(buff, &token[1], sizeof(buff));
					if (strLen - 2 >= 0 && (size_t)(strLen - 2) < strlen(buff) && buff[strLen - 2] == '"')
					{
						buff[strLen - 2] = '\0';
						done = true;
					}
				}
				if (!done)
				{
					token = getNextToken(getSepsQuote());
					if (strlen(token) > 1 && token[1] != '\t')
					{
						strlcat_local(buff, " ", sizeof(buff));
						strlcat_local(buff, token, sizeof(buff));
					}
					else
					{
						Int buflen = (Int)strlen(buff);
						if (buflen > 0 && buff[buflen - 1] == '"')
							buff[buflen - 1] = '\0';
					}
				}
				return buff;
			}
		}
		return std::string();
	}

	// INI::getNextAsciiString — verbatim.
	std::string getNextAsciiString()
	{
		static char buff[INI_MAX_CHARS_PER_LINE];
		buff[0] = 0;
		const char* token = getNextTokenOrNull();
		if (token != nullptr)
		{
			if (token[0] != '"')
			{
				return token;
			}
			else
			{
				if (strlen(token) > 1)
				{
					strlcpy_local(buff, &token[1], sizeof(buff));
				}
				token = getNextTokenOrNull(getSepsQuote());
				if (token)
				{
					if (strlen(token) > 1 && token[1] != '\t')
					{
						strlcat_local(buff, " ", sizeof(buff));
					}
					strlcat_local(buff, token, sizeof(buff));
					return buff;
				}
				else
				{
					Int len = (Int)strlen(buff);
					if (len && buff[len - 1] == '"')
					{
						buff[len - 1] = 0;
					}
					return buff;
				}
			}
		}
		return std::string();
	}

	// findFieldParse — verbatim.
	static INIFieldParseProc findFieldParse(const FieldParse* parseTable, const char* token, int& offset, const void*& userData)
	{
		const FieldParse* parse = parseTable;
		for (; parse->token; ++parse)
		{
			if (strcmp(parse->token, token) == 0)
			{
				offset = parse->offset;
				userData = parse->userData;
				return parse->parse;
			}
		}
		if (!parse->token && parse->parse)
		{
			offset = parse->offset;
			userData = token;
			return parse->parse;
		}
		else
		{
			return nullptr;
		}
	}

	// INI::initFromINIMulti (single table) — verbatim, release semantics:
	// unknown fields are skipped silently.
	void initFromINI(void* what, const FieldParse* parseTable)
	{
		if (what == nullptr)
			fail("INI_INVALID_PARAMS");

		Bool done = false;
		while (!done)
		{
			readLine();
			const char* field = strtok(m_buffer, getSeps());
			if (field)
			{
				if (stricmp(field, getEndToken()) == 0)
				{
					done = true;
				}
				else
				{
					int offset = 0;
					const void* userData = nullptr;
					INIFieldParseProc parse = findFieldParse(parseTable, field, offset, userData);
					if (parse)
					{
						parse(this, what, (char*)what + offset, userData);
					}
					// unknown field: DEBUG_CRASH is a no-op in release.
				}
			}
			if (done == false && isEOF() == true)
			{
				fail("INI_MISSING_END_TOKEN");
			}
		}
	}

	// INI::parseBitString32 — verbatim.
	static void parseBitString32(INI* ini, void* /*instance*/, void* store, const void* userData)
	{
		const char* const* flagList = (const char* const*)userData;
		UnsignedInt* bits = (UnsignedInt*)store;
		if (flagList == nullptr || flagList[0] == nullptr)
		{
			ini->fail("INI_INVALID_NAME_LIST");
		}

		Bool foundNormal = false;
		Bool foundAddOrSub = false;
		for (const char* token = ini->getNextTokenOrNull(); token; token = ini->getNextTokenOrNull())
		{
			if (stricmp(token, "NONE") == 0)
			{
				if (foundNormal || foundAddOrSub)
				{
					ini->fail("INI_INVALID_NAME_LIST");
				}
				*bits = 0;
				break;
			}
			if (token[0] == '+')
			{
				if (foundNormal)
				{
					ini->fail("INI_INVALID_NAME_LIST");
				}
				Int bitIndex = scanIndexList(token + 1, flagList, ini->getFilename(), ini->getLineNum());
				*bits |= (1 << bitIndex);
				foundAddOrSub = true;
			}
			else if (token[0] == '-')
			{
				if (foundNormal)
				{
					ini->fail("INI_INVALID_NAME_LIST");
				}
				Int bitIndex = scanIndexList(token + 1, flagList, ini->getFilename(), ini->getLineNum());
				*bits &= ~(1 << bitIndex);
				foundAddOrSub = true;
			}
			else
			{
				if (foundAddOrSub)
				{
					ini->fail("INI_INVALID_NAME_LIST");
				}
				if (!foundNormal)
					*bits = 0;
				Int bitIndex = scanIndexList(token, flagList, ini->getFilename(), ini->getLineNum());
				*bits |= (1 << bitIndex);
				foundNormal = true;
			}
		}
	}

	// INI::parseBitInInt32 — verbatim.
	static void parseBitInInt32(INI* ini, void* /*instance*/, void* store, const void* userData)
	{
		UnsignedInt* s = (UnsignedInt*)store;
		UnsignedInt mask = (UnsignedInt)(uintptr_t)userData;
		if (scanBool(ini->getNextToken(), ini->getFilename(), ini->getLineNum()))
			*s |= mask;
		else
			*s &= ~mask;
	}
};

// ----------------------------------------------------------------------------
// the three block types
// ----------------------------------------------------------------------------

// Generic field parse procs (transcribed from INI.cpp); defined below,
// after the reader.
static void parseRealField(INI* ini, void* instance, void* store, const void* userData);
static void parseIntField(INI* ini, void* instance, void* store, const void* userData);
static void parseBoolField(INI* ini, void* instance, void* store, const void* userData);
static void parseIndexListField(INI* ini, void* instance, void* store, const void* userData);
static void parseAsciiStringField(INI* ini, void* instance, void* store, const void* userData);
static void parseAngleReal(INI* ini, void* instance, void* store, const void* userData);
static void parseVelocityReal(INI* ini, void* instance, void* store, const void* userData);
static void parseAccelerationReal(INI* ini, void* instance, void* store, const void* userData);
static void parseAngularVelocityReal(INI* ini, void* instance, void* store, const void* userData);
static void parseDurationReal(INI* ini, void* instance, void* store, const void* userData);
static void parseDurationUnsignedInt(INI* ini, void* instance, void* store, const void* userData);

// --- Armor (complete) --------------------------------------------------------

enum
{
	DAMAGE_NUM_TYPES = 38
};

static const char* const DamageTypeNames[DAMAGE_NUM_TYPES + 1] = {
	"EXPLOSION",
	"CRUSH",
	"ARMOR_PIERCING",
	"SMALL_ARMS",
	"GATTLING",
	"RADIATION",
	"FLAME",
	"LASER",
	"SNIPER",
	"POISON",
	"HEALING",
	"UNRESISTABLE",
	"WATER",
	"DEPLOY",
	"SURRENDER",
	"HACK",
	"KILL_PILOT",
	"PENALTY",
	"FALLING",
	"MELEE",
	"DISARM",
	"HAZARD_CLEANUP",
	"PARTICLE_BEAM",
	"TOPPLING",
	"INFANTRY_MISSILE",
	"AURORA_BOMB",
	"LAND_MINE",
	"JET_MISSILES",
	"STEALTHJET_MISSILES",
	"MOLOTOV_COCKTAIL",
	"COMANCHE_VULCAN",
	"SUBDUAL_MISSILE",
	"SUBDUAL_VEHICLE",
	"SUBDUAL_BUILDING",
	"SUBDUAL_UNRESISTABLE",
	"MICROWAVE",
	"KILL_GARRISONED",
	"STATUS",
	nullptr};

struct ArmorTemplate
{
	Real m_damageCoefficient[DAMAGE_NUM_TYPES];

	void clear()
	{
		for (int i = 0; i < DAMAGE_NUM_TYPES; i++)
		{
			m_damageCoefficient[i] = 1.0f;
		}
	}
};

// ArmorTemplate::parseArmorCoefficients — verbatim.
static void parseArmorCoefficients(INI* ini, void* instance, void* /*store*/, const void* /*userData*/)
{
	ArmorTemplate* self = (ArmorTemplate*)instance;
	const char* damageName = ini->getNextToken();
	Real pct = scanPercentToReal(ini->getNextToken(), ini->getFilename(), ini->getLineNum());
	if (stricmp(damageName, "Default") == 0)
	{
		for (int i = 0; i < DAMAGE_NUM_TYPES; i++)
		{
			self->m_damageCoefficient[i] = pct;
		}
		return;
	}
	int dt = scanIndexList(damageName, DamageTypeNames, ini->getFilename(), ini->getLineNum());
	self->m_damageCoefficient[dt] = pct;
}

struct ArmorStore
{
	std::vector<std::string> names;
	std::vector<ArmorTemplate> templates;

	size_t find(const std::string& name) const
	{
		for (size_t i = 0; i < names.size(); ++i)
			if (names[i] == name)
				return i;
		return SIZE_MAX;
	}

	// ArmorStore::parseArmorDefinition — verbatim.
	void parseDefinition(INI* ini)
	{
		static const FieldParse myFieldParse[] = {
			{"Armor", parseArmorCoefficients, nullptr, 0},
			{nullptr, nullptr, nullptr, 0}};

		const char* c = ini->getNextToken();
		size_t index = find(c);
		if (index == SIZE_MAX)
		{
			names.push_back(c);
			templates.push_back(ArmorTemplate());
			index = names.size() - 1;
		}
		ArmorTemplate& armorTmpl = templates[index];
		armorTmpl.clear();
		ini->initFromINI(&armorTmpl, myFieldParse);
	}
};

// --- Locomotor (complete) ----------------------------------------------------

static const char* const TheLocomotorSurfaceTypeNames[] = {"GROUND", "WATER", "CLIFF", "AIR", "RUBBLE", nullptr};
static const char* const TheLocomotorBehaviorZNames[] = {
	"NO_Z_MOTIVE_FORCE", "SEA_LEVEL", "SURFACE_RELATIVE_HEIGHT", "ABSOLUTE_HEIGHT",
	"FIXED_SURFACE_RELATIVE_HEIGHT", "FIXED_ABSOLUTE_HEIGHT", "FIXED_RELATIVE_TO_GROUND_AND_BUILDINGS",
	"RELATIVE_TO_HIGHEST_LAYER", nullptr};
static const char* const TheLocomotorAppearanceNames[] = {
	"TWO_LEGS", "FOUR_WHEELS", "TREADS", "HOVER", "THRUST", "WINGS", "CLIMBER", "OTHER", "MOTORCYCLE", nullptr};
static const char* const TheLocomotorPriorityNames[] = {"MOVES_BACK", "MOVES_MIDDLE", "MOVES_FRONT", nullptr};

static const Real BIGNUM = 99999.0f;

struct LocomotorTemplate
{
	LocomotorTemplate* m_nextOverride = nullptr;
	bool m_isOverride = false;
	std::string m_name;

	UnsignedInt m_surfaces;
	Real m_maxSpeed;
	Real m_maxSpeedDamaged;
	Real m_maxTurnRate;
	Real m_maxTurnRateDamaged;
	Real m_acceleration;
	Real m_accelerationDamaged;
	Real m_lift;
	Real m_liftDamaged;
	Real m_braking;
	Real m_minSpeed;
	Real m_minTurnSpeed;
	Real m_preferredHeight;
	Real m_preferredHeightDamping;
	Real m_circlingRadius;
	Real m_extra2DFriction;
	Real m_speedLimitZ;
	Real m_maxThrustAngle;
	Int m_behaviorZ;
	Int m_appearance;
	Int m_movePriority;
	Real m_accelPitchLimit;
	Real m_decelPitchLimit;
	Real m_bounceKick;
	Real m_pitchStiffness;
	Real m_rollStiffness;
	Real m_pitchDamping;
	Real m_rollDamping;
	Real m_thrustRoll;
	Real m_wobbleRate;
	Real m_minWobble;
	Real m_maxWobble;
	Real m_pitchByZVelCoef;
	Real m_forwardVelCoef;
	Real m_lateralVelCoef;
	Real m_forwardAccelCoef;
	Real m_lateralAccelCoef;
	Real m_uniformAxialDamping;
	Real m_turnPivotOffset;
	bool m_apply2DFrictionWhenAirborne;
	bool m_downhillOnly;
	bool m_allowMotiveForceWhileAirborne;
	bool m_locomotorWorksWhenDead;
	Int m_airborneTargetingHeight;
	bool m_stickToGround;
	bool m_canMoveBackward;
	bool m_hasSuspension;
	Real m_wheelTurnAngle;
	Real m_maximumWheelExtension;
	Real m_maximumWheelCompression;
	Real m_closeEnoughDist;
	bool m_isCloseEnoughDist3D;
	Real m_ultraAccurateSlideIntoPlaceFactor;
	Real m_wanderWidthFactor;
	Real m_wanderLengthFactor;
	Real m_wanderAboutPointRadius;
	Real m_rudderCorrectionDegree;
	Real m_rudderCorrectionRate;
	Real m_elevatorCorrectionDegree;
	Real m_elevatorCorrectionRate;

	// LocomotorTemplate::LocomotorTemplate — verbatim defaults.
	LocomotorTemplate()
	{
		m_maxSpeedDamaged = -1.0f;
		m_maxTurnRateDamaged = -1.0f;
		m_accelerationDamaged = -1.0f;
		m_liftDamaged = -1.0f;

		m_surfaces = 0;
		m_maxSpeed = 0.0f;
		m_maxTurnRate = 0.0f;
		m_acceleration = 0.0f;
		m_lift = 0.0f;
		m_braking = BIGNUM;
		m_minSpeed = 0.0f;
		m_minTurnSpeed = BIGNUM;
		m_behaviorZ = 0; // Z_NO_Z_MOTIVE_FORCE
		m_appearance = 7; // LOCO_OTHER
		m_movePriority = 1; // LOCO_MOVES_MIDDLE
		m_preferredHeight = 0;
		m_preferredHeightDamping = 1.0f;
		m_circlingRadius = 0;

		m_maxThrustAngle = 0;
		m_speedLimitZ = 999999.0f;
		m_extra2DFriction = 0.0f;

		m_accelPitchLimit = 0;
		m_decelPitchLimit = 0;
		m_bounceKick = 0;

		m_pitchStiffness = 0.1f;
		m_rollStiffness = 0.1f;
		m_pitchDamping = 0.9f;
		m_rollDamping = 0.9f;
		m_forwardVelCoef = 0;
		m_pitchByZVelCoef = 0;
		m_thrustRoll = 0.0f;
		m_wobbleRate = 0.0f;
		m_minWobble = 0.0f;
		m_maxWobble = 0.0f;
		m_lateralVelCoef = 0;
		m_forwardAccelCoef = 0;
		m_lateralAccelCoef = 0;
		m_uniformAxialDamping = 1.0f;
		m_turnPivotOffset = 0;
		m_apply2DFrictionWhenAirborne = false;
		m_downhillOnly = false;
		m_allowMotiveForceWhileAirborne = false;
		m_locomotorWorksWhenDead = false;
		m_airborneTargetingHeight = INT_MAX;
		m_stickToGround = false;
		m_canMoveBackward = false;
		m_hasSuspension = false;
		m_wheelTurnAngle = 0;
		m_maximumWheelExtension = 0;
		m_maximumWheelCompression = 0;
		m_closeEnoughDist = 1.0f;
		m_isCloseEnoughDist3D = false;
		m_ultraAccurateSlideIntoPlaceFactor = 0.0f;

		m_wanderWidthFactor = 0.0f;
		m_wanderLengthFactor = 1.0f;
		m_wanderAboutPointRadius = 0.0f;

		m_rudderCorrectionDegree = 0.0f;
		m_rudderCorrectionRate = 0.0f;
		m_elevatorCorrectionDegree = 0.0f;
		m_elevatorCorrectionRate = 0.0f;
	}

	~LocomotorTemplate() { delete m_nextOverride; }

	void setNextOverride(LocomotorTemplate* next) { m_nextOverride = next; }
	void markAsOverride() { m_isOverride = true; }

	const LocomotorTemplate* friend_getFinalOverride() const
	{
		if (m_nextOverride)
			return m_nextOverride->friend_getFinalOverride();
		return this;
	}
	LocomotorTemplate* friend_getFinalOverride()
	{
		if (m_nextOverride)
			return m_nextOverride->friend_getFinalOverride();
		return this;
	}
	void friend_setName(const char* n) { m_name = n; }

	// LocomotorTemplate::validate — verbatim (Zero Hour branch; the WINGS
	// healing is a silent release-build heal, the THRUST violations throw).
	void validate(INI* ini)
	{
		if (m_maxSpeedDamaged < 0.0f)
			m_maxSpeedDamaged = m_maxSpeed;
		if (m_maxTurnRateDamaged < 0.0f)
			m_maxTurnRateDamaged = m_maxTurnRate;
		if (m_accelerationDamaged < 0.0f)
			m_accelerationDamaged = m_acceleration;
		if (m_liftDamaged < 0.0f)
			m_liftDamaged = m_lift;

		if (m_appearance == 5) // LOCO_WINGS
		{
			if (m_minSpeed <= 0.0f)
				m_minSpeed = 0.01f;
			if (m_minTurnSpeed <= 0.0f)
				m_minTurnSpeed = 0.01f;
		}

		if (m_appearance == 4) // LOCO_THRUST
		{
			if (m_behaviorZ != 0 || m_lift != 0.0f || m_liftDamaged != 0.0f)
			{
				ini->fail("INI_INVALID_DATA");
			}
			if (m_maxSpeed <= 0.0f)
				m_maxSpeed = 0.01f;
			if (m_maxSpeedDamaged <= 0.0f)
				m_maxSpeedDamaged = 0.01f;
			if (m_minSpeed <= 0.0f)
				m_minSpeed = 0.01f;
		}
	}
};

// parseFrictionPerSec (Locomotor.cpp) — verbatim.
static void parseFrictionPerSec(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	Real fricPerSec = scanReal(ini->getNextToken(), ini->getFilename(), ini->getLineNum());
	Real fricPerFrame = fricPerSec * SECONDS_PER_LOGICFRAME_REAL;
	*(Real*)store = fricPerFrame;
}

#define OFF(field) ((int)offsetof(LocomotorTemplate, field))

// LocomotorTemplate::getFieldParse — verbatim field table.
static const FieldParse* getLocomotorFieldParse()
{
	static const FieldParse TheFieldParse[] = {
		{"Surfaces", INI::parseBitString32, TheLocomotorSurfaceTypeNames, OFF(m_surfaces)},
		{"Speed", parseVelocityReal, nullptr, OFF(m_maxSpeed)},
		{"SpeedDamaged", parseVelocityReal, nullptr, OFF(m_maxSpeedDamaged)},
		{"TurnRate", parseAngularVelocityReal, nullptr, OFF(m_maxTurnRate)},
		{"TurnRateDamaged", parseAngularVelocityReal, nullptr, OFF(m_maxTurnRateDamaged)},
		{"Acceleration", parseAccelerationReal, nullptr, OFF(m_acceleration)},
		{"AccelerationDamaged", parseAccelerationReal, nullptr, OFF(m_accelerationDamaged)},
		{"Lift", parseAccelerationReal, nullptr, OFF(m_lift)},
		{"LiftDamaged", parseAccelerationReal, nullptr, OFF(m_liftDamaged)},
		{"Braking", parseAccelerationReal, nullptr, OFF(m_braking)},
		{"MinSpeed", parseVelocityReal, nullptr, OFF(m_minSpeed)},
		{"MinTurnSpeed", parseVelocityReal, nullptr, OFF(m_minTurnSpeed)},
		{"PreferredHeight", parseRealField, nullptr, OFF(m_preferredHeight)},
		{"PreferredHeightDamping", parseRealField, nullptr, OFF(m_preferredHeightDamping)},
		{"CirclingRadius", parseRealField, nullptr, OFF(m_circlingRadius)},
		{"Extra2DFriction", parseFrictionPerSec, nullptr, OFF(m_extra2DFriction)},
		{"SpeedLimitZ", parseVelocityReal, nullptr, OFF(m_speedLimitZ)},
		{"MaxThrustAngle", parseAngleReal, nullptr, OFF(m_maxThrustAngle)},
		{"ZAxisBehavior", parseIndexListField, TheLocomotorBehaviorZNames, OFF(m_behaviorZ)},
		{"Appearance", parseIndexListField, TheLocomotorAppearanceNames, OFF(m_appearance)},
		{"GroupMovementPriority", parseIndexListField, TheLocomotorPriorityNames, OFF(m_movePriority)},

		{"AccelerationPitchLimit", parseAngleReal, nullptr, OFF(m_accelPitchLimit)},
		{"DecelerationPitchLimit", parseAngleReal, nullptr, OFF(m_decelPitchLimit)},
		{"BounceAmount", parseAngularVelocityReal, nullptr, OFF(m_bounceKick)},
		{"PitchStiffness", parseRealField, nullptr, OFF(m_pitchStiffness)},
		{"RollStiffness", parseRealField, nullptr, OFF(m_rollStiffness)},
		{"PitchDamping", parseRealField, nullptr, OFF(m_pitchDamping)},
		{"RollDamping", parseRealField, nullptr, OFF(m_rollDamping)},
		{"ThrustRoll", parseRealField, nullptr, OFF(m_thrustRoll)},
		{"ThrustWobbleRate", parseRealField, nullptr, OFF(m_wobbleRate)},
		{"ThrustMinWobble", parseRealField, nullptr, OFF(m_minWobble)},
		{"ThrustMaxWobble", parseRealField, nullptr, OFF(m_maxWobble)},
		{"PitchInDirectionOfZVelFactor", parseRealField, nullptr, OFF(m_pitchByZVelCoef)},
		{"ForwardVelocityPitchFactor", parseRealField, nullptr, OFF(m_forwardVelCoef)},
		{"LateralVelocityRollFactor", parseRealField, nullptr, OFF(m_lateralVelCoef)},
		{"ForwardAccelerationPitchFactor", parseRealField, nullptr, OFF(m_forwardAccelCoef)},
		{"LateralAccelerationRollFactor", parseRealField, nullptr, OFF(m_lateralAccelCoef)},
		{"UniformAxialDamping", parseRealField, nullptr, OFF(m_uniformAxialDamping)},
		{"TurnPivotOffset", parseRealField, nullptr, OFF(m_turnPivotOffset)},
		{"Apply2DFrictionWhenAirborne", parseBoolField, nullptr, OFF(m_apply2DFrictionWhenAirborne)},
		{"DownhillOnly", parseBoolField, nullptr, OFF(m_downhillOnly)},
		{"AllowAirborneMotiveForce", parseBoolField, nullptr, OFF(m_allowMotiveForceWhileAirborne)},
		{"LocomotorWorksWhenDead", parseBoolField, nullptr, OFF(m_locomotorWorksWhenDead)},
		{"AirborneTargetingHeight", parseIntField, nullptr, OFF(m_airborneTargetingHeight)},
		{"StickToGround", parseBoolField, nullptr, OFF(m_stickToGround)},
		{"CanMoveBackwards", parseBoolField, nullptr, OFF(m_canMoveBackward)},
		{"HasSuspension", parseBoolField, nullptr, OFF(m_hasSuspension)},
		{"FrontWheelTurnAngle", parseAngleReal, nullptr, OFF(m_wheelTurnAngle)},
		{"MaximumWheelExtension", parseRealField, nullptr, OFF(m_maximumWheelExtension)},
		{"MaximumWheelCompression", parseRealField, nullptr, OFF(m_maximumWheelCompression)},
		{"CloseEnoughDist", parseRealField, nullptr, OFF(m_closeEnoughDist)},
		{"CloseEnoughDist3D", parseBoolField, nullptr, OFF(m_isCloseEnoughDist3D)},
		{"SlideIntoPlaceTime", parseDurationReal, nullptr, OFF(m_ultraAccurateSlideIntoPlaceFactor)},

		{"WanderWidthFactor", parseRealField, nullptr, OFF(m_wanderWidthFactor)},
		{"WanderLengthFactor", parseRealField, nullptr, OFF(m_wanderLengthFactor)},
		{"WanderAboutPointRadius", parseRealField, nullptr, OFF(m_wanderAboutPointRadius)},

		{"RudderCorrectionDegree", parseRealField, nullptr, OFF(m_rudderCorrectionDegree)},
		{"RudderCorrectionRate", parseRealField, nullptr, OFF(m_rudderCorrectionRate)},
		{"ElevatorCorrectionDegree", parseRealField, nullptr, OFF(m_elevatorCorrectionDegree)},
		{"ElevatorCorrectionRate", parseRealField, nullptr, OFF(m_elevatorCorrectionRate)},
		{nullptr, nullptr, nullptr, 0}};

	return TheFieldParse;
}

struct LocomotorStore
{
	std::vector<LocomotorTemplate*> templates;

	~LocomotorStore()
	{
		for (LocomotorTemplate* t : templates)
			delete t;
	}

	LocomotorTemplate* findLocomotorTemplate(const std::string& name)
	{
		for (LocomotorTemplate* t : templates)
			if (t->m_name == name)
				return t;
		return nullptr;
	}

	// LocomotorStore::newOverride — verbatim.
	static LocomotorTemplate* newOverride(LocomotorTemplate* locoTemplate)
	{
		if (locoTemplate == nullptr)
			return nullptr;
		LocomotorTemplate* newTemplate = new LocomotorTemplate;
		*newTemplate = *locoTemplate;
		locoTemplate->setNextOverride(newTemplate);
		newTemplate->markAsOverride();
		return newTemplate;
	}

	// LocomotorStore::parseLocomotorTemplateDefinition — verbatim.
	void parseDefinition(INI* ini)
	{
		Bool isOverride = false;
		const char* token = ini->getNextToken();

		LocomotorTemplate* loco = findLocomotorTemplate(token);
		if (loco)
		{
			if (ini->getLoadType() == INI_LOAD_CREATE_OVERRIDES)
			{
				loco = newOverride(loco->friend_getFinalOverride());
			}
			isOverride = true;
		}
		else
		{
			loco = new LocomotorTemplate;
			if (ini->getLoadType() == INI_LOAD_CREATE_OVERRIDES)
			{
				loco->markAsOverride();
			}
		}

		loco->friend_setName(token);
		ini->initFromINI(loco, getLocomotorFieldParse());
		loco->validate(ini);

		if (!isOverride)
			templates.push_back(loco);
	}
};

// --- Weapon (ported subset) --------------------------------------------------

static const char* const TheDeathNames[] = {
	"NORMAL", "NONE", "CRUSHED", "BURNED", "EXPLODED", "POISONED", "TOPPLED", "FLOODED",
	"SUICIDED", "LASERED", "DETONATED", "SPLATTED", "POISONED_BETA", "EXTRA_2", "EXTRA_3",
	"EXTRA_4", "EXTRA_5", "EXTRA_6", "EXTRA_7", "EXTRA_8", "POISONED_GAMMA", nullptr};
static const char* const TheWeaponReloadNames[] = {"YES", "NO", "RETURN_TO_BASE", nullptr};
static const char* const TheWeaponPrefireNames[] = {"PER_SHOT", "PER_ATTACK", "PER_CLIP", nullptr};
static const char* const TheWeaponAffectsMaskNames[] = {
	"SELF", "ALLIES", "ENEMIES", "NEUTRALS", "SUICIDE", "NOT_SIMILAR", "NOT_AIRBORNE", nullptr};

enum
{
	WEAPON_ANTI_AIRBORNE_VEHICLE = 0x01,
	WEAPON_ANTI_GROUND = 0x02,
	WEAPON_ANTI_BALLISTIC_MISSILE = 0x40,
};

struct WeaponTemplate
{
	WeaponTemplate* m_nextTemplate = nullptr;
	std::string m_name = "NoNameWeapon";

	Real m_primaryDamage;
	Real m_primaryDamageRadius;
	Real m_secondaryDamage;
	Real m_secondaryDamageRadius;
	Real m_attackRange;
	Real m_minimumAttackRange;
	Real m_aimDelta;
	Int m_damageType;
	Int m_deathType;
	Real m_weaponSpeed;
	Real m_minWeaponSpeed;
	Bool m_isScaleWeaponSpeed;
	Real m_weaponRecoil;
	std::string m_projectileName;
	Int m_clipSize;
	Int m_clipReloadTime;
	Int m_minDelayBetweenShots;
	Int m_maxDelayBetweenShots;
	Int m_shotsPerBarrel;
	Bool m_damageDealtAtSelfPosition;
	Int m_affectsMask;
	Int m_antiMask;
	Int m_reloadType;
	UnsignedInt m_preAttackDelay;
	Int m_prefireType;
	Real m_historicBonusRadius;
	Bool m_leechRangeWeapon;
	Bool m_isShowsAmmoPips;
	std::string m_laserName;

	// The subset of WeaponTemplate::WeaponTemplate defaults.
	WeaponTemplate()
	{
		m_primaryDamage = 0.0f;
		m_primaryDamageRadius = 0.0f;
		m_secondaryDamage = 0.0f;
		m_secondaryDamageRadius = 0.0f;
		m_attackRange = 0.0f;
		m_minimumAttackRange = 0.0f;
		m_aimDelta = 0.0f;
		m_damageType = 0; // DAMAGE_EXPLOSION
		m_deathType = 0; // DEATH_NORMAL
		m_weaponSpeed = 999999.0f;
		m_minWeaponSpeed = 999999.0f;
		m_isScaleWeaponSpeed = false;
		m_weaponRecoil = 0.0f;
		m_clipSize = 0;
		m_clipReloadTime = 0;
		m_minDelayBetweenShots = 0;
		m_maxDelayBetweenShots = 0;
		m_shotsPerBarrel = 1;
		m_damageDealtAtSelfPosition = false;
		m_affectsMask = (0x02 | 0x04 | 0x08);
		m_antiMask = WEAPON_ANTI_GROUND;
		m_reloadType = 0; // AUTO_RELOAD
		m_preAttackDelay = 0;
		m_prefireType = 0; // PREFIRE_PER_SHOT
		m_historicBonusRadius = 0.0f;
		m_leechRangeWeapon = false;
		m_isShowsAmmoPips = false;
	}

	bool isNone(const std::string& s) const { return !s.empty() && stricmp(s.c_str(), "None") == 0; }
};

// WeaponTemplate::parseShotDelay — verbatim.
static void parseShotDelay(INI* ini, void* instance, void* /*store*/, const void* /*userData*/)
{
	WeaponTemplate* self = (WeaponTemplate*)instance;
	static const char* MIN_LABEL = "Min";
	static const char* MAX_LABEL = "Max";

	const char* token = ini->getNextToken(ini->getSepsColon());
	if (stricmp(token, MIN_LABEL) == 0)
	{
		self->m_minDelayBetweenShots =
			scanInt(ini->getNextToken(ini->getSepsColon()), ini->getFilename(), ini->getLineNum());
		token = ini->getNextToken(ini->getSepsColon());
		if (stricmp(token, MAX_LABEL) != 0)
		{
			self->m_maxDelayBetweenShots = self->m_minDelayBetweenShots;
		}
		else
		{
			self->m_maxDelayBetweenShots =
				scanInt(ini->getNextToken(ini->getSepsColon()), ini->getFilename(), ini->getLineNum());
		}
	}
	else
	{
		self->m_minDelayBetweenShots = scanInt(token, ini->getFilename(), ini->getLineNum());
		self->m_maxDelayBetweenShots = self->m_minDelayBetweenShots;
	}

	self->m_minDelayBetweenShots = (Int)Ceilf(ConvertDurationFromMsecsToFrames((Real)self->m_minDelayBetweenShots));
	self->m_maxDelayBetweenShots = (Int)Ceilf(ConvertDurationFromMsecsToFrames((Real)self->m_maxDelayBetweenShots));
}

// DamageTypeFlags::parseSingleBitFromINI — verbatim: stores the bit INDEX.
static void parseSingleBitFromINI(INI* ini, void* /*instance*/, void* store, const void* userData)
{
	const char* token = ini->getNextToken();
	Int bitIndex = scanIndexList(token, (const char* const*)userData, ini->getFilename(), ini->getLineNum());
	*(Int*)store = bitIndex;
}

#define WOFF(field) ((int)offsetof(WeaponTemplate, field))

// The ported subset of WeaponTemplate::TheWeaponTemplateFieldParseTable.
static const FieldParse* getWeaponFieldParse()
{
	static const FieldParse TheWeaponTemplateFieldParseTable[] = {
		{"PrimaryDamage", parseRealField, nullptr, WOFF(m_primaryDamage)},
		{"PrimaryDamageRadius", parseRealField, nullptr, WOFF(m_primaryDamageRadius)},
		{"SecondaryDamage", parseRealField, nullptr, WOFF(m_secondaryDamage)},
		{"SecondaryDamageRadius", parseRealField, nullptr, WOFF(m_secondaryDamageRadius)},
		{"AttackRange", parseRealField, nullptr, WOFF(m_attackRange)},
		{"MinimumAttackRange", parseRealField, nullptr, WOFF(m_minimumAttackRange)},
		{"AcceptableAimDelta", parseAngleReal, nullptr, WOFF(m_aimDelta)},
		{"DamageType", parseSingleBitFromINI, DamageTypeNames, WOFF(m_damageType)},
		{"DeathType", parseIndexListField, TheDeathNames, WOFF(m_deathType)},
		{"WeaponSpeed", parseVelocityReal, nullptr, WOFF(m_weaponSpeed)},
		{"MinWeaponSpeed", parseVelocityReal, nullptr, WOFF(m_minWeaponSpeed)},
		{"ScaleWeaponSpeed", parseBoolField, nullptr, WOFF(m_isScaleWeaponSpeed)},
		{"WeaponRecoil", parseAngleReal, nullptr, WOFF(m_weaponRecoil)},
		{"ProjectileObject", parseAsciiStringField, nullptr, WOFF(m_projectileName)},
		{"ClipSize", parseIntField, nullptr, WOFF(m_clipSize)},
		{"ClipReloadTime", parseDurationUnsignedInt, nullptr, WOFF(m_clipReloadTime)},
		{"DelayBetweenShots", parseShotDelay, nullptr, 0},
		{"ShotsPerBarrel", parseIntField, nullptr, WOFF(m_shotsPerBarrel)},
		{"DamageDealtAtSelfPosition", parseBoolField, nullptr, WOFF(m_damageDealtAtSelfPosition)},
		{"RadiusDamageAffects", INI::parseBitString32, TheWeaponAffectsMaskNames, WOFF(m_affectsMask)},
		{"AntiAirborneVehicle", INI::parseBitInInt32, (const void*)WEAPON_ANTI_AIRBORNE_VEHICLE, WOFF(m_antiMask)},
		{"AntiGround", INI::parseBitInInt32, (const void*)WEAPON_ANTI_GROUND, WOFF(m_antiMask)},
		{"AntiBallisticMissile", INI::parseBitInInt32, (const void*)WEAPON_ANTI_BALLISTIC_MISSILE, WOFF(m_antiMask)},
		{"AutoReloadsClip", parseIndexListField, TheWeaponReloadNames, WOFF(m_reloadType)},
		{"PreAttackDelay", parseDurationUnsignedInt, nullptr, WOFF(m_preAttackDelay)},
		{"PreAttackType", parseIndexListField, TheWeaponPrefireNames, WOFF(m_prefireType)},
		{"HistoricBonusRadius", parseRealField, nullptr, WOFF(m_historicBonusRadius)},
		{"LeechRangeWeapon", parseBoolField, nullptr, WOFF(m_leechRangeWeapon)},
		{"ShowsAmmoPips", parseBoolField, nullptr, WOFF(m_isShowsAmmoPips)},
		{"LaserName", parseAsciiStringField, nullptr, WOFF(m_laserName)},
		{nullptr, nullptr, nullptr, 0}};

	return TheWeaponTemplateFieldParseTable;
}

struct WeaponStore
{
	std::vector<WeaponTemplate*> templates;

	~WeaponStore()
	{
		for (WeaponTemplate* t : templates)
			delete t;
	}

	WeaponTemplate* findWeaponTemplatePrivate(const std::string& name)
	{
		for (WeaponTemplate* t : templates)
			if (t->m_name == name)
				return t;
		return nullptr;
	}

	// WeaponStore::newWeaponTemplate (the name is never empty here).
	WeaponTemplate* newWeaponTemplate(const char* name)
	{
		WeaponTemplate* wt = new WeaponTemplate;
		wt->m_name = name;
		templates.push_back(wt);
		return wt;
	}

	// WeaponStore::newOverride — verbatim: the copy chains BACK to the
	// original and is never registered, so overrides stay unreachable
	// (lookups keep the base template).
	static WeaponTemplate* newOverride(WeaponTemplate* weaponTemplate)
	{
		if (!weaponTemplate)
			return nullptr;
		WeaponTemplate* wt = new WeaponTemplate;
		(*wt) = (*weaponTemplate);
		wt->m_nextTemplate = weaponTemplate;
		return wt;
	}

	// WeaponStore::parseWeaponTemplateDefinition — verbatim.
	void parseDefinition(INI* ini)
	{
		std::string name;
		const char* c = ini->getNextToken();
		name = c;

		WeaponTemplate* weapon = findWeaponTemplatePrivate(name);
		if (weapon)
		{
			if (ini->getLoadType() == INI_LOAD_CREATE_OVERRIDES)
				weapon = newOverride(weapon);
			else
			{
				// DEBUG_CRASH is a no-op in release builds; return without
				// consuming the block.
				return;
			}
		}
		else
		{
			weapon = newWeaponTemplate(name.c_str());
		}

		ini->initFromINI(weapon, getWeaponFieldParse());

		if (weapon->isNone(weapon->m_projectileName))
			weapon->m_projectileName.clear();
	}
};

// ----------------------------------------------------------------------------
// load driver (the INI::load block dispatch for the three types)
// ----------------------------------------------------------------------------

struct GameData
{
	ArmorStore armor;
	LocomotorStore loco;
	WeaponStore weapon;

	void parseBlock(INI* ini, const char* token)
	{
		if (strcmp(token, "Armor") == 0)
			armor.parseDefinition(ini);
		else if (strcmp(token, "Locomotor") == 0)
			loco.parseDefinition(ini);
		else if (strcmp(token, "Weapon") == 0)
			weapon.parseDefinition(ini);
		else
			ini->fail("INI_UNKNOWN_TOKEN");
	}
};

// INI::load — verbatim loop (the byte source is the file contents here).
static UnsignedInt loadIni(INI& ini, const char* filename, const std::vector<uint8_t>& data, LoadType loadType, GameData& game)
{
	ini.m_data = data;
	ini.m_readBufferNext = 0;
	ini.m_filename = filename;
	ini.m_loadType = loadType;
	ini.m_lineNum = 0;
	ini.m_buffer[0] = 0;
	ini.m_cend = 0;
	ini.m_endOfFile = false;

	while (ini.m_endOfFile == false)
	{
		ini.readLine();
		const char* token = strtok(ini.m_buffer, INI::getSeps());
		if (token)
		{
			game.parseBlock(&ini, token);
		}
	}
	return 1;
}

// ----------------------------------------------------------------------------
// dump
// ----------------------------------------------------------------------------

static FILE* g_out = nullptr;

static void dumpString(const std::string& s)
{
	fprintf(g_out, "%zu:", s.size());
	fwrite(s.data(), 1, s.size(), g_out);
}

static void dumpHexFloat(Real v)
{
	uint32_t bits;
	memcpy(&bits, &v, sizeof(bits));
	fprintf(g_out, "0x%08x", bits);
}

static void dumpArmor(const ArmorStore& store)
{
	fprintf(g_out, "dump armor %zu\n", store.templates.size());
	for (size_t i = 0; i < store.templates.size(); ++i)
	{
		fprintf(g_out, "armor ");
		dumpString(store.names[i]);
		fprintf(g_out, "\n");
		for (int d = 0; d < DAMAGE_NUM_TYPES; ++d)
		{
			fprintf(g_out, "  damage_coefficient[%d] ", d);
			dumpHexFloat(store.templates[i].m_damageCoefficient[d]);
			fprintf(g_out, "\n");
		}
	}
}

static void dumpLocomotor(const LocomotorStore& store)
{
	fprintf(g_out, "dump locomotor %zu\n", store.templates.size());
	for (const LocomotorTemplate* base : store.templates)
	{
		const LocomotorTemplate* finalT = base->friend_getFinalOverride();
		int depth = 0;
		for (const LocomotorTemplate* n = base; n; n = n->m_nextOverride)
			depth++;
		fprintf(g_out, "locomotor ");
		dumpString(finalT->m_name);
		fprintf(g_out, " depth %d override %d\n", depth, finalT->m_isOverride ? 1 : 0);
		fprintf(g_out, "  surfaces 0x%x\n", finalT->m_surfaces);
		fprintf(g_out, "  max_speed "); dumpHexFloat(finalT->m_maxSpeed); fprintf(g_out, "\n");
		fprintf(g_out, "  max_speed_damaged "); dumpHexFloat(finalT->m_maxSpeedDamaged); fprintf(g_out, "\n");
		fprintf(g_out, "  max_turn_rate "); dumpHexFloat(finalT->m_maxTurnRate); fprintf(g_out, "\n");
		fprintf(g_out, "  max_turn_rate_damaged "); dumpHexFloat(finalT->m_maxTurnRateDamaged); fprintf(g_out, "\n");
		fprintf(g_out, "  acceleration "); dumpHexFloat(finalT->m_acceleration); fprintf(g_out, "\n");
		fprintf(g_out, "  acceleration_damaged "); dumpHexFloat(finalT->m_accelerationDamaged); fprintf(g_out, "\n");
		fprintf(g_out, "  lift "); dumpHexFloat(finalT->m_lift); fprintf(g_out, "\n");
		fprintf(g_out, "  lift_damaged "); dumpHexFloat(finalT->m_liftDamaged); fprintf(g_out, "\n");
		fprintf(g_out, "  braking "); dumpHexFloat(finalT->m_braking); fprintf(g_out, "\n");
		fprintf(g_out, "  min_speed "); dumpHexFloat(finalT->m_minSpeed); fprintf(g_out, "\n");
		fprintf(g_out, "  min_turn_speed "); dumpHexFloat(finalT->m_minTurnSpeed); fprintf(g_out, "\n");
		fprintf(g_out, "  preferred_height "); dumpHexFloat(finalT->m_preferredHeight); fprintf(g_out, "\n");
		fprintf(g_out, "  preferred_height_damping "); dumpHexFloat(finalT->m_preferredHeightDamping); fprintf(g_out, "\n");
		fprintf(g_out, "  circling_radius "); dumpHexFloat(finalT->m_circlingRadius); fprintf(g_out, "\n");
		fprintf(g_out, "  extra_2d_friction "); dumpHexFloat(finalT->m_extra2DFriction); fprintf(g_out, "\n");
		fprintf(g_out, "  speed_limit_z "); dumpHexFloat(finalT->m_speedLimitZ); fprintf(g_out, "\n");
		fprintf(g_out, "  max_thrust_angle "); dumpHexFloat(finalT->m_maxThrustAngle); fprintf(g_out, "\n");
		fprintf(g_out, "  behavior_z %d\n", finalT->m_behaviorZ);
		fprintf(g_out, "  appearance %d\n", finalT->m_appearance);
		fprintf(g_out, "  move_priority %d\n", finalT->m_movePriority);
		fprintf(g_out, "  accel_pitch_limit "); dumpHexFloat(finalT->m_accelPitchLimit); fprintf(g_out, "\n");
		fprintf(g_out, "  decel_pitch_limit "); dumpHexFloat(finalT->m_decelPitchLimit); fprintf(g_out, "\n");
		fprintf(g_out, "  bounce_kick "); dumpHexFloat(finalT->m_bounceKick); fprintf(g_out, "\n");
		fprintf(g_out, "  pitch_stiffness "); dumpHexFloat(finalT->m_pitchStiffness); fprintf(g_out, "\n");
		fprintf(g_out, "  roll_stiffness "); dumpHexFloat(finalT->m_rollStiffness); fprintf(g_out, "\n");
		fprintf(g_out, "  pitch_damping "); dumpHexFloat(finalT->m_pitchDamping); fprintf(g_out, "\n");
		fprintf(g_out, "  roll_damping "); dumpHexFloat(finalT->m_rollDamping); fprintf(g_out, "\n");
		fprintf(g_out, "  thrust_roll "); dumpHexFloat(finalT->m_thrustRoll); fprintf(g_out, "\n");
		fprintf(g_out, "  wobble_rate "); dumpHexFloat(finalT->m_wobbleRate); fprintf(g_out, "\n");
		fprintf(g_out, "  min_wobble "); dumpHexFloat(finalT->m_minWobble); fprintf(g_out, "\n");
		fprintf(g_out, "  max_wobble "); dumpHexFloat(finalT->m_maxWobble); fprintf(g_out, "\n");
		fprintf(g_out, "  pitch_by_z_vel_coef "); dumpHexFloat(finalT->m_pitchByZVelCoef); fprintf(g_out, "\n");
		fprintf(g_out, "  forward_vel_coef "); dumpHexFloat(finalT->m_forwardVelCoef); fprintf(g_out, "\n");
		fprintf(g_out, "  lateral_vel_coef "); dumpHexFloat(finalT->m_lateralVelCoef); fprintf(g_out, "\n");
		fprintf(g_out, "  forward_accel_coef "); dumpHexFloat(finalT->m_forwardAccelCoef); fprintf(g_out, "\n");
		fprintf(g_out, "  lateral_accel_coef "); dumpHexFloat(finalT->m_lateralAccelCoef); fprintf(g_out, "\n");
		fprintf(g_out, "  uniform_axial_damping "); dumpHexFloat(finalT->m_uniformAxialDamping); fprintf(g_out, "\n");
		fprintf(g_out, "  turn_pivot_offset "); dumpHexFloat(finalT->m_turnPivotOffset); fprintf(g_out, "\n");
		fprintf(g_out, "  apply_2d_friction_when_airborne %d\n", finalT->m_apply2DFrictionWhenAirborne ? 1 : 0);
		fprintf(g_out, "  downhill_only %d\n", finalT->m_downhillOnly ? 1 : 0);
		fprintf(g_out, "  allow_motive_force_while_airborne %d\n", finalT->m_allowMotiveForceWhileAirborne ? 1 : 0);
		fprintf(g_out, "  locomotor_works_when_dead %d\n", finalT->m_locomotorWorksWhenDead ? 1 : 0);
		fprintf(g_out, "  airborne_targeting_height %d\n", finalT->m_airborneTargetingHeight);
		fprintf(g_out, "  stick_to_ground %d\n", finalT->m_stickToGround ? 1 : 0);
		fprintf(g_out, "  can_move_backward %d\n", finalT->m_canMoveBackward ? 1 : 0);
		fprintf(g_out, "  has_suspension %d\n", finalT->m_hasSuspension ? 1 : 0);
		fprintf(g_out, "  wheel_turn_angle "); dumpHexFloat(finalT->m_wheelTurnAngle); fprintf(g_out, "\n");
		fprintf(g_out, "  maximum_wheel_extension "); dumpHexFloat(finalT->m_maximumWheelExtension); fprintf(g_out, "\n");
		fprintf(g_out, "  maximum_wheel_compression "); dumpHexFloat(finalT->m_maximumWheelCompression); fprintf(g_out, "\n");
		fprintf(g_out, "  close_enough_dist "); dumpHexFloat(finalT->m_closeEnoughDist); fprintf(g_out, "\n");
		fprintf(g_out, "  is_close_enough_dist_3d %d\n", finalT->m_isCloseEnoughDist3D ? 1 : 0);
		fprintf(g_out, "  ultra_accurate_slide_into_place_factor "); dumpHexFloat(finalT->m_ultraAccurateSlideIntoPlaceFactor); fprintf(g_out, "\n");
		fprintf(g_out, "  wander_width_factor "); dumpHexFloat(finalT->m_wanderWidthFactor); fprintf(g_out, "\n");
		fprintf(g_out, "  wander_length_factor "); dumpHexFloat(finalT->m_wanderLengthFactor); fprintf(g_out, "\n");
		fprintf(g_out, "  wander_about_point_radius "); dumpHexFloat(finalT->m_wanderAboutPointRadius); fprintf(g_out, "\n");
		fprintf(g_out, "  rudder_correction_degree "); dumpHexFloat(finalT->m_rudderCorrectionDegree); fprintf(g_out, "\n");
		fprintf(g_out, "  rudder_correction_rate "); dumpHexFloat(finalT->m_rudderCorrectionRate); fprintf(g_out, "\n");
		fprintf(g_out, "  elevator_correction_degree "); dumpHexFloat(finalT->m_elevatorCorrectionDegree); fprintf(g_out, "\n");
		fprintf(g_out, "  elevator_correction_rate "); dumpHexFloat(finalT->m_elevatorCorrectionRate); fprintf(g_out, "\n");
	}
}

static void dumpWeapon(const WeaponStore& store)
{
	fprintf(g_out, "dump weapon %zu\n", store.templates.size());
	for (const WeaponTemplate* t : store.templates)
	{
		fprintf(g_out, "weapon ");
		dumpString(t->m_name);
		fprintf(g_out, "\n");
		fprintf(g_out, "  primary_damage "); dumpHexFloat(t->m_primaryDamage); fprintf(g_out, "\n");
		fprintf(g_out, "  primary_damage_radius "); dumpHexFloat(t->m_primaryDamageRadius); fprintf(g_out, "\n");
		fprintf(g_out, "  secondary_damage "); dumpHexFloat(t->m_secondaryDamage); fprintf(g_out, "\n");
		fprintf(g_out, "  secondary_damage_radius "); dumpHexFloat(t->m_secondaryDamageRadius); fprintf(g_out, "\n");
		fprintf(g_out, "  attack_range "); dumpHexFloat(t->m_attackRange); fprintf(g_out, "\n");
		fprintf(g_out, "  minimum_attack_range "); dumpHexFloat(t->m_minimumAttackRange); fprintf(g_out, "\n");
		fprintf(g_out, "  aim_delta "); dumpHexFloat(t->m_aimDelta); fprintf(g_out, "\n");
		fprintf(g_out, "  damage_type %d\n", t->m_damageType);
		fprintf(g_out, "  death_type %d\n", t->m_deathType);
		fprintf(g_out, "  weapon_speed "); dumpHexFloat(t->m_weaponSpeed); fprintf(g_out, "\n");
		fprintf(g_out, "  min_weapon_speed "); dumpHexFloat(t->m_minWeaponSpeed); fprintf(g_out, "\n");
		fprintf(g_out, "  is_scale_weapon_speed %d\n", t->m_isScaleWeaponSpeed ? 1 : 0);
		fprintf(g_out, "  weapon_recoil "); dumpHexFloat(t->m_weaponRecoil); fprintf(g_out, "\n");
		fprintf(g_out, "  projectile_name "); dumpString(t->m_projectileName); fprintf(g_out, "\n");
		fprintf(g_out, "  clip_size %d\n", t->m_clipSize);
		fprintf(g_out, "  clip_reload_time %u\n", t->m_clipReloadTime);
		fprintf(g_out, "  min_delay_between_shots %d\n", t->m_minDelayBetweenShots);
		fprintf(g_out, "  max_delay_between_shots %d\n", t->m_maxDelayBetweenShots);
		fprintf(g_out, "  shots_per_barrel %d\n", t->m_shotsPerBarrel);
		fprintf(g_out, "  damage_dealt_at_self_position %d\n", t->m_damageDealtAtSelfPosition ? 1 : 0);
		fprintf(g_out, "  affects_mask 0x%x\n", t->m_affectsMask);
		fprintf(g_out, "  anti_mask 0x%x\n", t->m_antiMask);
		fprintf(g_out, "  reload_type %d\n", t->m_reloadType);
		fprintf(g_out, "  pre_attack_delay %u\n", t->m_preAttackDelay);
		fprintf(g_out, "  prefire_type %d\n", t->m_prefireType);
		fprintf(g_out, "  historic_bonus_radius "); dumpHexFloat(t->m_historicBonusRadius); fprintf(g_out, "\n");
		fprintf(g_out, "  leech_range_weapon %d\n", t->m_leechRangeWeapon ? 1 : 0);
		fprintf(g_out, "  is_shows_ammo_pips %d\n", t->m_isShowsAmmoPips ? 1 : 0);
		fprintf(g_out, "  laser_name "); dumpString(t->m_laserName); fprintf(g_out, "\n");
	}
}

// ----------------------------------------------------------------------------
// generic field parse procs (transcribed from INI.cpp)
// ----------------------------------------------------------------------------

static void parseRealField(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	const char* token = ini->getNextToken();
	*(Real*)store = scanReal(token, ini->getFilename(), ini->getLineNum());
}

static void parseIntField(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	const char* token = ini->getNextToken();
	*(Int*)store = scanInt(token, ini->getFilename(), ini->getLineNum());
}

static void parseBoolField(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	*(Bool*)store = scanBool(ini->getNextToken(), ini->getFilename(), ini->getLineNum());
}

static void parseIndexListField(INI* ini, void* /*instance*/, void* store, const void* userData)
{
	const char* token = ini->getNextToken();
	*(Int*)store = scanIndexList(token, (const char* const*)userData, ini->getFilename(), ini->getLineNum());
}

static void parseAsciiStringField(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	*(std::string*)store = ini->getNextAsciiString();
}

static void parseAngleReal(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	const char* token = ini->getNextToken();
	const Real RADS_PER_DEGREE = PI / 180.0f;
	*(Real*)store = scanReal(token, ini->getFilename(), ini->getLineNum()) * RADS_PER_DEGREE;
}

static void parseVelocityReal(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	const char* token = ini->getNextToken();
	*(Real*)store = ConvertVelocityInSecsToFrames(scanReal(token, ini->getFilename(), ini->getLineNum()));
}

static void parseAccelerationReal(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	const char* token = ini->getNextToken();
	*(Real*)store = ConvertAccelerationInSecsToFrames(scanReal(token, ini->getFilename(), ini->getLineNum()));
}

static void parseAngularVelocityReal(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	const char* token = ini->getNextToken();
	*(Real*)store = ConvertAngularVelocityInDegreesPerSecToRadsPerFrame(scanReal(token, ini->getFilename(), ini->getLineNum()));
}

static void parseDurationReal(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	Real val = scanReal(ini->getNextToken(), ini->getFilename(), ini->getLineNum());
	*(Real*)store = ConvertDurationFromMsecsToFrames(val);
}

static void parseDurationUnsignedInt(INI* ini, void* /*instance*/, void* store, const void* /*userData*/)
{
	UnsignedInt val = scanUnsignedInt(ini->getNextToken(), ini->getFilename(), ini->getLineNum());
	*(UnsignedInt*)store = (UnsignedInt)Ceilf(ConvertDurationFromMsecsToFrames((Real)val));
}

// ----------------------------------------------------------------------------
// main
// ----------------------------------------------------------------------------

static std::vector<uint8_t> readFile(const std::string& path)
{
	FILE* f = fopen(path.c_str(), "rb");
	if (!f)
	{
		fprintf(stderr, "cannot open %s\n", path.c_str());
		exit(1);
	}
	fseek(f, 0, SEEK_END);
	long size = ftell(f);
	fseek(f, 0, SEEK_SET);
	std::vector<uint8_t> data((size_t)size);
	if (size > 0 && fread(data.data(), 1, (size_t)size, f) != (size_t)size)
	{
		fprintf(stderr, "cannot read %s\n", path.c_str());
		exit(1);
	}
	fclose(f);
	return data;
}

// LF -> CRLF in memory (the fixture files stay LF-only in git).
static std::vector<uint8_t> toCrlf(const std::vector<uint8_t>& data)
{
	std::vector<uint8_t> out;
	out.reserve(data.size() * 2);
	for (uint8_t c : data)
	{
		if (c == '\n')
			out.push_back('\r');
		out.push_back(c);
	}
	return out;
}

struct Harness
{
	std::string fixturesDir;
	GameData game;
	INI ini;

	void run(const char* name, LoadType loadType, bool crlf = false)
	{
		std::vector<uint8_t> data = readFile(fixturesDir + "/" + name);
		if (crlf)
			data = toCrlf(data);
		try
		{
			UnsignedInt files = loadIni(ini, name, data, loadType, game);
			fprintf(g_out, "file %s %s ok %u\n", name, loadTypeName(loadType), files);
		}
		catch (const IniError& e)
		{
			fprintf(g_out, "file %s %s error %s line %u\n", name, loadTypeName(loadType), e.kind, e.line);
		}
	}
};

int main(int argc, char** argv)
{
	if (argc < 2)
	{
		fprintf(stderr, "usage: %s <output-file> [fixtures-dir]\n", argv[0]);
		return 1;
	}
	const char* outPath = argv[1];
	std::string fixturesDir = argc > 2 ? argv[2] : "crates/rzh-ini/tests/fixtures";

	g_out = fopen(outPath, "wb");
	if (!g_out)
	{
		fprintf(stderr, "cannot open %s for writing\n", outPath);
		return 1;
	}

	fprintf(g_out, "# rzh-ini golden dump v1\n");

	Harness harness;
	harness.fixturesDir = fixturesDir;

	harness.run("armor.ini", INI_LOAD_OVERWRITE);
	harness.run("locomotor.ini", INI_LOAD_OVERWRITE);
	harness.run("weapon.ini", INI_LOAD_OVERWRITE);
	harness.run("override.ini", INI_LOAD_CREATE_OVERRIDES);
	harness.run("edge.ini", INI_LOAD_OVERWRITE, /*crlf=*/true);

	harness.run("bad_unknown_block.ini", INI_LOAD_OVERWRITE);
	harness.run("bad_missing_end.ini", INI_LOAD_OVERWRITE);
	harness.run("bad_value.ini", INI_LOAD_OVERWRITE);
	harness.run("bad_bool.ini", INI_LOAD_OVERWRITE);
	harness.run("bad_index.ini", INI_LOAD_OVERWRITE);
	harness.run("bad_bitstring_mix.ini", INI_LOAD_OVERWRITE);
	harness.run("bad_loco_thrust.ini", INI_LOAD_OVERWRITE);
	harness.run("bad_no_value.ini", INI_LOAD_OVERWRITE);
	harness.run("weapon_dup.ini", INI_LOAD_OVERWRITE);

	dumpArmor(harness.game.armor);
	dumpLocomotor(harness.game.loco);
	dumpWeapon(harness.game.weapon);
	fprintf(g_out, "end\n");

	fclose(g_out);
	return 0;
}
