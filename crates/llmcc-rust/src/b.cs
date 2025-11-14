/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
//
//  Copyright (C) 2023-2024 Microsoft. All rights reserved.
//
//  Module Name:
//      xast_symbols.cs
//
//  Abstract:
//      Classes for AST symbols.
//      Error range: XR2600-2699
//
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Text;
using Astred.Common;
using Astred.GraphLib;

namespace Astred.AstLib
{
    /// <summary>
    /// Represents a symbol defined in the AST.
    /// </summary>
    [PythonApiExpose]
    public class AstSymbol : IComparable<AstSymbol>
    {
        private protected static readonly IReadOnlyList<AstSymbol> emptySymbolList = new List<AstSymbol>();

        /// <summary>
        /// The AST token identifier for the node defining this symbol.
        /// </summary>
        public AstToken Tokenid { get; }

        /// <summary>
        /// The name of the symbol.
        /// </summary>
        public virtual string Name { get; private set; }

        /// <summary>
        /// The mangled name of the symbol, which may be used to differentiate between overloads.
        /// </summary>
        public string Mangled { get; private set; }

        /// <summary>
        /// Must contain all information necessary to differentiate between overloads.
        /// </summary>
        internal AstTypedName TypedName { get; set; }

        /// <summary>
        /// The symbol that this symbol is a field of, if any.
        /// [TYPE SYSTEM] remove setter
        /// </summary>
        public virtual AstSymbol FieldOf { get; internal set; }

        /// <summary>
        /// The symbol that represents the type of this symbol, if known.
        /// </summary>
        public virtual AstSymbol TypeOf { get; internal set; }

        /// <summary>
        /// The symbol that represents the previous instance of this symbol, if any.
        /// </summary>
        public AstSymbol Previous { get; internal set; }

        /// <summary>
        /// The origin point of the symbol in the source code.
        /// </summary>
        public AstPoint Origin { get; internal set; }

        /// <summary>
        /// Indicates whether the symbol is overloaded.
        /// </summary>
        public bool IsOverloaded { get => Overloads == null ? false : true; }

        /// <summary>
        /// Indicates whether the symbol is generic/template.
        /// </summary>
        public bool IsGeneric { get; internal set; }

        /// <summary>
        /// Indicates whether the symbol cannot be extended or modified by code.
        /// </summary>
        public bool IsLocked { get; internal set; }

        /// <summary>
        /// The list of overloads for this symbol, if it is an overloaded symbol.
        /// </summary>
        public List<AstSymbol> Overloads { get; internal set; }

        /// <summary>
        /// The list of nested types within this symbol, if any. Compound types like classes, structs, and enums have nested types.
        /// </summary>
        public List<AstSymbol> NestedTypes { get; internal set; }

        /// <summary>
        /// The base symbol from which this symbol was derived.
        /// </summary>
        public AstSymbol BaseSymbol { get; internal set; }

        /// <summary>
        /// The scope defined by this symbol.
        /// </summary>
        public AstScope Scope { get { return _scope; } internal set { /*Xred.Assert(!IsPrimitive());*/ _scope = value; } }
        private AstScope _scope;

        /// <summary>
        /// The scope this symbol was originally defined in.
        /// </summary>
        public AstScope ParentScope { get; private set; }

        /// <summary>
        /// The edit status of the symbol.
        /// </summary>
        public AstSymbolEditStatus EditStatus { get; internal set; }

        /// <summary>
        /// The AST node that defines this symbol.
        /// </summary>
        internal AstNodeBase Defined { get; private set; }

        private List<AstNodeBase> definingAsts;

        private static readonly IReadOnlyList<AstNodeBase> emptyDefiningAstsList = new List<AstNodeBase>();

        /// <summary>
        /// The blocks defining this symbol.
        /// </summary>
        internal IReadOnlyList<AstNodeBase> DefiningAsts { get { return definingAsts ?? emptyDefiningAstsList; } }

        /// <summary>
        /// The primary block defining this symbol.
        /// </summary>
        [DebuggerBrowsable(DebuggerBrowsableState.Never)]
        public Block PrimaryBlock { get; private set; }

        private List<Block> blocks;

        private static readonly IReadOnlyList<Block> emptyBlockList = new List<Block>();

        /// <summary>
        /// The blocks defining this symbol.
        /// </summary>
        public IReadOnlyList<Block> Blocks { get { return blocks ?? emptyBlockList; } }

        /// <summary>
        /// The Symbolic Graph associated with this Symbol.
        /// </summary>
        public SymNode SymGraph { get; set; }

        /// <summary>
        /// A unique identifier for this symbol.  Should be used *ONLY* for debugging and testing.
        /// </summary>
        public int DebugId { get; }

        private static int ids = 0;

        /// <summary>
        /// Resets the static counter so that future symbols have Ids starting at 1.
        /// Should be used *ONLY* for debugging and testing.
        /// </summary>
        public static void ResetIdBase()
        {
            ids = 0;
        }

        /// <summary>
        /// Verify that this primitive type retains no references to other AST objects.
        /// </summary>
        internal void AssertPrimitive()
        {
            Debug.Assert(FieldOf == null);
            Debug.Assert(TypeOf == null);
            Debug.Assert(Scope == null);
            Debug.Assert(Overloads == null);
            Debug.Assert(NestedTypes == null);
            Debug.Assert(BaseSymbol == null);
            Debug.Assert(Defined == null);
            Debug.Assert(definingAsts == null);
            Debug.Assert(SymGraph == null);
            Debug.Assert(PrimaryBlock == null);
            Debug.Assert(blocks == null);
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="AstSymbol"/> class with the specified token identifier and name.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        /// <param name="mangled">The mangled name of the symbol, used for unique identification.</param>
        /// <param name="idAsMangled">Flag indicating if the id should be used as the mangled name.</param>
        internal AstSymbol(AstScope parentScope, AstToken tokenid, string name, string mangled = null, bool idAsMangled = false)
        {
            this.ParentScope = parentScope;
            this.Tokenid = tokenid;
            this.Name = name;
            this.Mangled = mangled != null ? mangled : (idAsMangled ? ids.ToString() : name);
            this.Origin = AstPoint.Zero;
            this.DebugId = ++ids;
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="AstSymbol"/> class with the specified token identifier and name.
        /// </summary>
        /// <param name="ptid">The primitive type id for the symbol.</param>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        /// <param name="mangled">The mangled name of the symbol.</param>
        internal AstSymbol(AstPrimitiveType ptid, AstToken tokenid, string name, string mangled = null)
        {
            this.Tokenid = tokenid;
            this.Name = name;
            this.Mangled = mangled ?? name;
            this.Origin = AstPoint.Zero;
            this.DebugId = (int)ptid;
            this.IsLocked = true;
            Debug.Assert(ptid < 0);
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="AstSymbol"/> class with the specified name.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="name">The name of the symbol.</param>
        internal AstSymbol(AstScope parentScope, String name)
        {
            this.ParentScope = parentScope;
            this.Name = name;
            this.Mangled = name;
            this.DebugId = ++ids;
            this.Origin = AstPoint.Zero;
        }

        internal AstSymbol(AstToken tokenid, string name)
        {
            this.Tokenid = tokenid;
            this.Name = name;
            this.DebugId = ++ids;
            this.Origin = AstPoint.Zero;
        }

        internal AstSymbol(AstToken tokenid)
        {
            this.Tokenid = tokenid;
            this.DebugId = ++ids;
            this.Origin = AstPoint.Zero;
        }


        internal void Reset()
        {
            BaseSymbol = null;
            blocks = null;
            PrimaryBlock = null;
            definingAsts = null;
            Defined = null;
            SymGraph = null;
            TypeOf = null;
            FieldOf = null;
            Scope = null;
        }

        /// <summary>
        /// Creates a copy of an existing <see cref="AstSymbol"/> instance, duplicating its state but with a unique identifier.
        /// </summary>
        internal AstSymbol Clone()
        {
            return new AstSymbol(this.ParentScope, this.Tokenid, this.Name, this.Mangled) {
                FieldOf = this.FieldOf,
                TypeOf = this.TypeOf,
                Scope = this.Scope,
                IsGeneric = this.IsGeneric,
                BaseSymbol = this.BaseSymbol,
                TypedName = this.TypedName,
                NestedTypes = this.NestedTypes,
                Overloads = this.Overloads,
                Previous = this.Previous,
                Origin = this.Origin
            };
        }

        /// <summary>
        /// Method to change the name of a symbol
        /// </summary>
        /// <param name="name">New name for the symbol</param>
        /// <param name="mangled">New mangled name for the symbol</param>
        /// <param name="typedName">New typed name for the symbol</param>
        internal void RenameSymbol(string name, string mangled, AstTypedName typedName)
        {
            Name = name;
            Mangled = mangled;
            TypedName = typedName;
        }

        /// <summary>
        /// Formats the symbol for display, with an optional indentation level.
        /// </summary>
        /// <param name="indent">The indentation level for formatting the symbol.</param>
        /// <returns>A formatted string representation of the symbol.</returns>
        public string Format(int indent)
        {
            var sb = new StringBuilder();
            sb.Append(' ', indent);
            sb.AppendFormat("[{0}] [{1}:{2}]", Tokenid, Name, DebugId);
            if (FieldOf != null) {
                sb.AppendFormat(" {0}:{1}.", FieldOf.Name, FieldOf.DebugId);
            }
            if (TypeOf != null) {
                sb.AppendFormat(" ({0}:{1})", TypeOf.Name, TypeOf.DebugId);
            }
            if (Scope != null) {
                sb.AppendFormat(" {{{0}}}.count={1}", Scope.DebugId, Scope.Count);
            }
            return sb.ToString();
        }

        /// <summary>
        /// Compares this instance with another <see cref="AstSymbol"/> and returns an integer that indicates
        /// whether this instance precedes, follows, or occurs in the same position in the sort order as the other <see cref="AstSymbol"/>.
        /// Symbols are sorted based on their Id.
        /// </summary>
        /// <param name="other">An <see cref="AstSymbol"/> to compare with this instance.</param>
        /// <returns>A value that indicates the relative order of the objects being compared.</returns>
        public int CompareTo(AstSymbol other)
        {
            int cmp = Name.CompareTo(other.Name);
            if (cmp != 0) {
                return cmp;
            }
            return DebugId - other.DebugId;
        }

        /// <summary>
        /// Returns a hash code for this instance.
        /// </summary>
        /// <returns>A hash code for this instance, suitable for use in hashing algorithms and data structures like a hash table.</returns>
        public override int GetHashCode()
        {
            return DebugId;
        }

        /// <summary>
        /// Prints the symbol to the console with an optional indentation level.
        /// </summary>
        /// <param name="indent">The indentation level for printing the symbol.</param>
        public void Print(int indent)
        {
            Console.WriteLine(Format(indent));
        }

        /// <summary>
        /// Returns a string that represents the current symbol, formatted for debugging.
        /// </summary>
        /// <returns>A string that represents the current symbol.</returns>
        public override string ToString()   // Format for debugging.
        {
            var sb = new StringBuilder();
            sb.AppendFormat("[{0}:{1}]", Name, DebugId);
            if (FieldOf != null) {
                sb.AppendFormat(".[{0}:{1}]", FieldOf.Name, FieldOf.DebugId);
            }
            if (TypeOf != null) {
                sb.AppendFormat(" ({0}:{1})", TypeOf.Name, TypeOf.DebugId);
            }
            return sb.ToString();
        }

        /// <summary>
        /// Adds a nested type to the list of nested types within this symbol.
        /// </summary>
        /// <param name="sym">The nested type symbol to add.</param>
        internal void AddNestedType(AstSymbol sym)
        {
            NestedTypes ??= [];
            NestedTypes.Add(sym);
        }

        /// <summary>
        /// Return true if this symbol is a primitive type from the language.
        /// </summary>
        /// <returns>true if the symbol is a primitive type; otherwise, false.</returns>
        public bool IsPrimitive()
        {
            return (DebugId < 0);
        }

        /// <summary>
        /// Addes a block to this symbol's list of blocks.
        /// </summary>
        /// <param name="newBlock">The block to add.</param>
        public void AddBlock(Block newBlock)
        {
            blocks ??= [];
            blocks.Add(newBlock);

            if (PrimaryBlock == null) {
                PrimaryBlock = newBlock;
            }
            else if ((newBlock.Ast == Defined || PrimaryBlock.Ast != Defined)) {
                PrimaryBlock = newBlock;
            }
        }

        /// <summary>
        /// Adds an <see cref="AstNodeBase"/> as a node that defines this symbol.
        /// </summary>
        /// <param name="node">The node to add.</param>
        public void AddDefined(AstNodeBase node)
        {
            definingAsts ??= [];
            definingAsts.Add(node);
            Defined = node;
        }

        /// <summary>
        /// Removes the given node from the nodes defining this symbol.
        /// </summary>
        /// <param name="node">The node to remove.</param>
        public void RemoveDefined(AstNodeBase node)
        {
            if (definingAsts != null) {
                definingAsts.Remove(node);
                if (node == Defined && definingAsts.Count > 0) {
                    Defined = definingAsts.Last();
                }
            }
        }

        internal void OverrideParentScope(AstScope parentScope)
        {
            this.ParentScope = parentScope;
        }
    }

    /// <summary>
    /// Represents a symbol that can have a type.
    /// </summary>
    public abstract class SymTyped : AstSymbol
    {
        private AstSymbol typeOf;

        /// <inheritdoc/>
        public override AstSymbol TypeOf { get => typeOf; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymTyped"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        internal SymTyped(AstScope parentScope, AstToken tokenid, string name)
            : base(parentScope, tokenid, name)
        {
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymTyped"/> class with the tokenid.
        /// For implicit symbols.
        /// </summary>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        internal SymTyped(AstToken tokenid, string name)
            : base(tokenid, name)
        {

        }

        /// <summary>
        /// Initializes the TypeOf property. Fails if it is already initialized.
        /// </summary>
        /// <param name="value">The symbol representing the type of this symbol.</param>
        internal void InitializeTypeOf(AstSymbol value)
        {
            Xred.Assert(typeOf == null);
            typeOf = value;
        }

        /// <summary>
        /// Overrides the value of the TypeOf property.
        /// </summary>
        /// <param name="value">The symbol representing the type of this symbol.</param>
        internal void OverrideTypeOf(AstSymbol value)
        {
            typeOf = value;
        }
    }

    /// <summary>
    /// Represents a variable or field symbol.
    /// </summary>
    public class SymField : SymTyped
    {
        private AstSymbol fieldOf;

        /// <inheritdoc/>
        public override AstSymbol FieldOf { get => fieldOf; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymField"/> class with the parent scope and node id.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        /// <param name="parentSymbol">The symbol this symbool is a field of. Null if none is passed.</param>
        internal SymField(AstScope parentScope, AstNodeId id, AstSymbol parentSymbol)
            : base(parentScope, id.Tokenid, id.Name)
        {
            this.fieldOf = parentSymbol;
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymField"/> class with the parent scope, node id and scope node.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="tokenId">The token id to be used in this symbol.</param>
        /// <param name="name">The name of this symbol.</param>
        /// <param name="parentSymbol">The symbol this symbool is a field of. Null if none is passed.</param>
        internal SymField(AstScope parentScope, AstToken tokenId, string name, AstSymbol parentSymbol)
            : base(parentScope, tokenId, name)
        {
            this.fieldOf = parentSymbol;
        }

        internal SymField(AstToken tokenid, string name, AstSymbol parentSymbol)
            : base(tokenid, name)
        {

        }

        /// <summary>
        /// Overrides the value of the FieldOf property.
        /// </summary>
        /// <param name="parent">The symbol this symbol if a field of.</param>
        internal void OverrideFieldOf(AstSymbol parent)
        {
            fieldOf = parent;
        }
    }

    /// <summary>
    /// Represents a function or method symbol.
    /// </summary>
    public class SymFunc : SymTyped
    {
        private AstSymbol fieldOf;

        /// <inheritdoc/>
        public override AstSymbol FieldOf { get => fieldOf; }

        /// <summary>
        /// Flag indicating whether this function overrides another.
        /// </summary>
        public virtual bool Overrides { get => false; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymFunc"/> class with the parent scope, node id and scope node.
        /// Creates a new <see cref="AstScope"/> for the symbol using the scope node as the root.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The parent scope.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        /// <param name="scopeNode">The scope node that should be used to create the scope for this symbol.</param>
        /// <param name="parentSymbol">The symbol this symbol is a field of. Null if none is passed.</param>
        internal SymFunc(AstScope parentScope, AstNodeId id, AstNodeScope scopeNode, AstSymbol parentSymbol = null)
            : base(parentScope, id.Tokenid, id.Name)
        {
            this.fieldOf = parentSymbol;
            this.Scope = new AstScope(scopeNode, this);
        }

        internal void OverrideFieldOf(AstSymbol value)
        {
            fieldOf = value;
        }
    }

    /// <summary>
    /// Represents a function that is overloadable.
    /// Contains extra properties to store overload information.
    /// </summary>
    public abstract class SymOverloadFunc : SymFunc
    {
        private List<AstSymbol> paramTypes;

        /// <summary>
        /// A list containing the type of each parameter of this function.
        /// </summary>
        public IReadOnlyList<AstSymbol> ParamTypes { get => paramTypes ?? emptySymbolList; }

        private List<AstSymbol> typeArgs;

        /// <summary>
        /// A list containing the symbol for each type argument of this function.
        /// </summary>
        public IReadOnlyList<AstSymbol> TypeArgs { get => typeArgs ?? emptySymbolList; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymFunc"/> class with the parent scope, node id and scope node.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The parent scope.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        /// <param name="scopeNode">The scope node that should be used to create the scope for this symbol.</param>
        /// <param name="parentSymbol">The symbol this symbol is a field of. Null if none is passed.</param>
        internal SymOverloadFunc(AstScope parentScope, AstNodeId id, AstNodeScope scopeNode, AstSymbol parentSymbol = null)
            : base(parentScope, id, scopeNode, parentSymbol)
        {
        }

        internal void AddParamType(AstSymbol parameter)
        {
            paramTypes ??= new List<AstSymbol>();
            paramTypes.Add(parameter);
        }

        internal void AddTypeArg(AstSymbol argument)
        {
            typeArgs ??= new List<AstSymbol>();
            typeArgs.Add(argument);
        }
    }

    /// <summary>
    /// Represents a label symbol.
    /// </summary>
    public class SymLabel : AstSymbol
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymLabel"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The parent scope.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymLabel(AstScope parentScope, AstNodeId id)
            : base(parentScope, id.Tokenid, id.Name)
        {
        }
    }

    /// <summary>
    /// Represents a module or namespace symbol.
    /// </summary>
    public class SymModule : AstSymbol
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymModule"/> class with the parent scope, node id and scope node.
        /// Creates a new <see cref="AstScope"/> for the symbol using the scope node as the root.
        /// </summary>
        /// <param name="parentScope">The parent scope.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        /// <param name="scopeNode">The scope node that should be used to create the scope for this symbol.</param>
        internal SymModule(AstScope parentScope, AstNodeId id, AstNodeScope scopeNode)
            : base(parentScope, id.Tokenid, id.Name)
        {
            this.Scope = new AstScope(scopeNode, this);
        }
    }

    /// <summary>
    /// Represents a type symbol.
    /// </summary>
    public abstract class SymType : AstSymbol
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymType"/> class with the parent scope and node id.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        internal SymType(AstScope parentScope, AstToken tokenid, string name)
            : base(parentScope, tokenid, name)
        {
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymType"/> class with the node id.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        internal SymType(AstToken tokenid, string name)
            : base(tokenid, name)
        {
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymType"/> class with the primitive type, token id and name.
        /// <param name="ptid">The primitive identifier for the symbol.</param>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        /// </summary>
        internal SymType(AstPrimitiveType ptid, AstToken tokenid, string name)
            : base(ptid, tokenid, name)
        {
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymType"/> class with the tokenid.
        /// Used for constructed types (tuples, arrays, etc.)
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// </summary>
        internal SymType(AstToken tokenid)
            : base(tokenid)
        {
        }
    }

    /// <summary>
    /// Represents a primitive type. Examples: int, bool, etc.
    /// </summary>
    public class SymPrimitiveType : SymType
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymPrimitiveType"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="ptid">The primitive identifier for the symbol.</param>
        /// <param name="tokenid">The token identifier for the symbol.</param>
        /// <param name="name">The name of the symbol.</param>
        internal SymPrimitiveType(AstPrimitiveType ptid, AstToken tokenid, string name)
            : base(ptid, tokenid, name)
        {
        }
    }

    /// <summary>
    /// Represents an array type.
    /// </summary>
    public class SymArrayType : SymType
    {
        /// <inheritdoc/>
        public override string Name { get => BuildName(); }

        /// <summary>
        /// The array dimensions.
        /// </summary>
        public int Dimensions { get; }

        internal new Bag<SymType> NestedTypes { get; } // [larissar] remove "new" after type system is finished

        internal SymArrayType(AstToken tokenid, SymType type, int dimensions = 1)
            : base(tokenid, "")
        {
            this.NestedTypes = new Bag<SymType>(type);
            this.Dimensions = dimensions;
        }

        internal virtual string BuildName()
        {
            var sb = new StringBuilder();
            for (int i = 0; i < Dimensions; i++) {
                sb.Append("[]");
            }
            AppendTypeName(sb);
            return sb.ToString();
        }

        internal static SymArrayType Create(AstNode node, SymType type)
        {
            var sym = new SymArrayType(node.Tokenid, type);
            node.Name = new AstNodeId(node, sym.Name, AstCategory.IdentifierTypeUse);
            node.Name.Symbol = sym;
            return sym;
        }

        internal void AppendTypeName(StringBuilder sb)
        {
            if (NestedTypes.Count == 1) {
                sb.Append(NestedTypes[0].Name);
            }
            else {
                sb.Append("(");
                for (int i = 0; i < NestedTypes.Count; i++) {
                    sb.Append(NestedTypes[i].Name);
                    if (i < NestedTypes.Count - 1) {
                        sb.Append(",");
                    }
                }
                sb.Append(")");
            }
        }
    }

    /// <summary>
    /// Represents a pointer type.
    /// </summary>
    public class SymPointerType : SymType
    {
        /// <inheritdoc/>
        public override string Name { get => $"*{NestedType.Name}"; }

        internal SymType NestedType { get; }

        internal SymPointerType(AstToken tokenid, SymType type)
            : base (tokenid)
        {
            this.NestedType = type;
        }

        internal static SymPointerType Create(AstNode node, SymType type)
        {
            var sym = new SymPointerType(node.Tokenid, type);
            node.Name = new AstNodeId(node, sym.Name, AstCategory.IdentifierTypeUse);
            node.Name.Symbol = sym;
            return sym;
        }
    }

    /// <summary>
    /// Represents a nullable type.
    /// </summary>
    public class SymNullableType : SymType
    {
        /// <inheritdoc/>
        public override string Name { get => $"?{NestedType.Name}"; }

        internal SymType NestedType { get; }

        internal SymNullableType(AstToken tokenid, SymType type)
            : base (tokenid)
        {
            this.NestedType = type;
        }

        internal static SymNullableType Create(AstNode node, SymType type)
        {
            var sym = new SymNullableType(node.Tokenid, type);
            node.Name = new AstNodeId(node, sym.Name, AstCategory.IdentifierTypeUse);
            node.Name.Symbol = sym;
            return sym;
        }
    }

    /// <summary>
    /// Represents a tuple type.
    /// </summary>
    public class SymTupleType : SymType
    {
        /// <inheritdoc/>
        public override string Name { get => BuildName(); }

        internal new Bag<SymType> NestedTypes { get; } // [larissar] remove "new" after type system is finished

        internal SymTupleType(AstNodeBase node, List<SymType> types)
            : base(node.Tokenid)
        {
            this.NestedTypes = new Bag<SymType>(types);
            this.Scope = new AstScope(null, this);
        }

        internal SymTupleType(AstNodeScope scopeNode, List<SymType> types)
            : base(scopeNode.Tokenid)
        {
            this.NestedTypes = new Bag<SymType>(types);
            this.Scope = new AstScope(scopeNode, this);
        }

        internal static SymTupleType Create(AstNodeScope node, List<SymType> types)
        {
            var sym = new SymTupleType(node, types);
            node.Name = new AstNodeId(node, sym.Name, AstCategory.IdentifierTypeUse);
            node.Name.Symbol = sym;
            node.Scope = sym.Scope;
            return sym;
        }

        internal static SymTupleType Create(AstNodeScope node, AstToken accessorToken, List<SymType> types, List<string> accessorNames)
        {
            var sym = Create(node, types);
            sym.CreateAccessors(accessorToken, accessorNames);
            return sym;
        }

        internal void CreateAccessors(AstToken tokenid, List<string> names)
        {
            // assert we will have as many accessors as nested types
            Xred.Assert(NestedTypes.Count == names.Count);

            for (int i = 0; i < NestedTypes.Count; i++) {
                var field = new SymField(tokenid, names[i], this);
                field.InitializeTypeOf(NestedTypes[i]); // [larissar]
                Scope.Add<SymField>(field);
            }
        }



        private string BuildName()
        {
            var sb = new StringBuilder("(");
            for (int i = 0; i < NestedTypes.Count; i++) {
                sb.Append(NestedTypes[i].Name);
                if (i < NestedTypes.Count - 1) {
                    sb.Append(",");
                }
            }
            sb.Append(")");
            return sb.ToString();
        }
    }

    /// <summary>
    /// Represents a a named type symbol. Examples: class, structs, enums, etc.
    /// </summary>
    public class SymNamedType : SymType
    {
        private AstSymbol fieldOf;

        /// <inheritdoc/>
        public override AstSymbol FieldOf { get => fieldOf; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymNamedType"/> class with the parent scope and node id.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The parent scope.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        /// <param name="parentSymbol">The symbol this symbol is a field of. Null if none is passed.</param>
        internal SymNamedType(AstScope parentScope, AstNodeId id, AstSymbol parentSymbol = null)
            : base(parentScope, id.Tokenid, id.Name)
        {
            this.fieldOf = parentSymbol;
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymNamedType"/> class with the parent scope, node id and scope node.
        /// Creates a new <see cref="AstScope"/> for the symbol using the scope node as the root.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The parent scope.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        /// <param name="scopeNode">The scope node that should be used to create the scope for this symbol.</param>
        /// <param name="parentSymbol">The symbol this symbool is a field of. Null if none is passed.</param>
        internal SymNamedType(AstScope parentScope, AstNodeId id, AstNodeScope scopeNode, AstSymbol parentSymbol = null)
            : base(parentScope, id.Tokenid, id.Name)
        {
            this.fieldOf = parentSymbol;
            this.Scope = new AstScope(scopeNode, this);
        }

        internal void OverrideFieldOf(AstSymbol value)
        {
            fieldOf = value;
        }
    }

    /// <summary>
    /// Represents a typedef or alias symbol.
    /// </summary>
    public class SymTypedef : SymType
    {
        private AstSymbol aliasFor;

        /// <summary>
        /// The symbol this typedef is an alias for.
        /// </summary>
        public AstSymbol AliasFor { get => aliasFor; }

        /// <summary>
        /// Returns the same value as AliasFor.
        /// </summary>
        public override AstSymbol TypeOf { get => AliasFor; } // [TYPESYSTEM] remove?

        /// <summary>
        /// Initializes a new instance of the <see cref="SymTypedef"/> class with the parent scope and node id.
        /// Setting the parent symbol is optional.
        /// </summary>
        /// <param name="parentScope">The parent scope.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymTypedef(AstScope parentScope, AstNodeId id)
            : base(parentScope, id.Tokenid, id.Name)
        {
        }

        internal void OverrideAliasFor(AstSymbol symbol)
        {
            aliasFor = symbol;
        }
    }

    /// <summary>
    ///  Represents a preprocessor directive symbol.
    /// </summary>
    public class SymPreproc : AstSymbol
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymPreproc"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymPreproc(AstScope parentScope, AstNodeId id)
            : base(parentScope, id.Tokenid, id.Name)
        {
        }
    }

    /// <summary>
    /// Represents a variable or field symbol.
    /// </summary>
    public class SymVar : SymTyped
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymVar"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymVar(AstScope parentScope, AstNodeId id)
            : base(parentScope, id.Tokenid, id.Name)
        {
        }
    }

    /// <summary>
    /// Represents an undefined symbol.
    /// </summary>
    internal class SymUndefined : AstSymbol
    {
        internal List<AstNodeBase> Nodes { get; private set; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymUndefined"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymUndefined(AstScope parentScope, AstNodeId id)
            : base(parentScope, id.Tokenid, id.Name)
        {
        }

        internal void AddNode(AstNodeBase node)
        {
            Nodes ??= new List<AstNodeBase>();
            Nodes.Add(node);
        }
    }

    /// <summary>
    /// Represents an undefined call symbol.
    /// </summary>
    internal class SymUndefCall : SymUndefined
    {
        private List<AstSymbol> argTypes;
        internal IReadOnlyList<AstSymbol> ArgTypes { get => argTypes ?? emptySymbolList; }

        private List<AstSymbol> genTypes;
        internal IReadOnlyList<AstSymbol> GenTypes { get => genTypes ?? emptySymbolList; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymUndefCall"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        /// <param name="argTypes"></param>
        /// <param name="genTypes"></param>
        internal SymUndefCall(AstScope parentScope, AstNodeId id, List<AstSymbol> argTypes, List<AstSymbol> genTypes)
            : base(parentScope, id)
        {
            if (argTypes != null && argTypes.Count > 0) {
                this.argTypes = argTypes;
            }

            if (genTypes != null && genTypes.Count > 0) {
                this.genTypes = genTypes;
            }
        }
    }

    /// <summary>
    /// Represents an undefined label symbol.
    /// </summary>
    internal class SymUndefLabel : SymUndefined
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymUndefLabel"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymUndefLabel(AstScope parentScope, AstNodeId id)
            : base(parentScope, id)
        {
        }
    }

    /// <summary>
    /// Represents an undefined call symbol.
    /// </summary>
    internal class SymUndefModule : SymUndefined
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymUndefModule"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymUndefModule(AstScope parentScope, AstNodeId id)
            : base(parentScope, id)
        {
        }
    }

    /// <summary>
    /// Represents an undefined type symbol.
    /// </summary>
    internal class SymUndefType : SymUndefined
    {
        internal List<AstSymbol> TypeFor { get; private set; }

        /// <summary>
        /// Initializes a new instance of the <see cref="SymUndefType"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymUndefType(AstScope parentScope, AstNodeId id)
            : base(parentScope, id)
        {
        }
    }

    /// <summary>
    /// Represents an undefined variable symbol.
    /// </summary>
    internal class SymUndefVar : SymUndefined
    {
        /// <summary>
        /// Initializes a new instance of the <see cref="SymUndefVar"/> class with the parent scope and node id.
        /// </summary>
        /// <param name="parentScope">The scope this symbol was originally declared in.</param>
        /// <param name="id">The <see cref="AstNodeId"/> this symbol should be created from.</param>
        internal SymUndefVar(AstScope parentScope, AstNodeId id)
            : base(parentScope, id)
        {
        }
    }

    /// <summary>
    /// Represents the edit status of an AST symbol.
    /// </summary>
    [PythonApiExpose]
    public enum AstSymbolEditStatus : byte
    {
        /// <summary>
        /// No edits have been made to the symbol.
        /// </summary>
        None = 0,

        /// <summary>
        /// The symbol has been inserted.
        /// </summary>
        Insert = 1,

        /// <summary>
        /// The symbol has been deleted.
        /// </summary>
        Delete = 2,
    }

    internal struct Bag<T> where T : class
    {
        object value;

        public T Value { get => value as T; }

        public List<T> List { get => value as List<T>; }

        public bool IsList { get => value is List<T>; }

        public int Count { get => (value is T) ? 1 : List.Count; }

        public Bag(T value)
        {
            this.value = value;
        }

        public Bag(List<T> list)
        {
            if (list.Count == 1) {
                this.value = list[0];
            }
            else {
                this.value = list;
            }
        }

        public T this[int index]
        {
            get {
                if (value is T) {
                    if (index == 0) {
                        return Value;
                    }
                    throw new IndexOutOfRangeException();
                }
                else {
                    return List[index];
                }
            }
        }

        public void Add(T newValue)
        {
            var list = value as List<T>;
            if (list != null) {
                list.Add(newValue);
            }
            else if (value == null) {
                value = newValue;
            }
            else {
                list = new List<T>();
                list.Add((T)value);
                list.Add(newValue);
                value = list;
            }
        }
    }

    /// <summary>
    /// Represents a symbol scope within an abstract syntax tree (AST).
    /// A scope is a context in which identifiers are defined and can be resolved.
    /// </summary>
    [PythonApiExpose]
    public class AstScope
    {
        /// <summary>
        /// Gets the symbol that owns this scope, if any.
        /// </summary>
        public AstSymbol Owner { get; internal set; }

        /// <summary>
        /// Gets the list of base scopes, such as parent classes, that this scope inherits from, if any.
        /// </summary>
        public List<AstScope> Bases { get; internal set; }

        /// <summary>
        /// Gets the number of symbols defined in this scope.
        /// </summary>
        public int Count { get { return (symbols != null) ? symbols.Count : 0; } }

        /// <summary>
        /// Gets a unique identifier for this scope.
        /// Should be used *ONLY* for debugging and testing.
        /// </summary>
        public int DebugId { get; }

        private Dictionary<(string, Type), Bag<AstSymbol>> symbols;
        private bool isCaseInsensitive;
        private static int ids = 0;
        private AstNodeScope root;
        internal AstNodeScope Root { get { return root; } }

        /// <summary>
        /// Resets the static counter so that future scaope have Ids starting at 1.
        /// Should be used *ONLY* for debugging and testing.
        /// </summary>
        public static void ResetIdBase()
        {
            ids = 0;
        }

        /// <summary>
        /// Returns a string that represents the current <see cref="AstScope"/>. Format is optimized for internal debugging.
        /// </summary>
        /// <returns>A string that represents the current <see cref="AstScope"/>.</returns>
        public override string ToString()
        {
            return String.Format("AstScope {0}:{1}, count={2}",
                DebugId, Owner != null ? Owner.ToString() : "[]", Count);
        }

        /// <summary>
        /// Initializes a new scope for containing systems.
        /// </summary>
        /// <param name="root">The AstNode that owns this scope.</param>
        /// <param name="isCaseInsensitive">Indicates whether the scope is case-insensitive.</param>
        public AstScope(AstNodeScope root, bool isCaseInsensitive = false)
        {
            this.root = root;
            this.Owner = null;
            this.symbols = null;
            this.isCaseInsensitive = isCaseInsensitive;
            this.Bases = null;
            this.DebugId = ++ids;
        }

        /// <summary>
        /// Initializes a new instance of the <see cref="AstScope"/> class with the specified owner symbol.
        /// </summary>
        /// <param name="root">The AstNode that owns this scope.</param>
        /// <param name="owner">The symbol that owns this scope.</param>
        /// <param name="isCaseInsensitive">Indicates whether the scope is case-insensitive.</param>
        internal AstScope(AstNodeScope root, AstSymbol owner, bool isCaseInsensitive = false)
        {
            this.root = root;
            this.Owner = owner;
            this.symbols = null;
            this.isCaseInsensitive = isCaseInsensitive;
            this.Bases = null;
            this.DebugId = ++ids;
        }

        internal AstNodeScope RootIfEmpty()
        {
            if (Count == 0 && Owner == null && root != null) {
                return root;
            }
            return null;
        }

        internal void SetRoot(AstNodeScope newRoot)
        {
            this.root = newRoot;
        }

        /// <summary>
        /// Gets an enumerable collection of symbols defined in this scope.
        /// </summary>
        public IEnumerable<AstSymbol> FindSymbols()
        {
            if (symbols != null) {
                foreach (var symbol in symbols.Values) {
                    if (symbol.Value is AstSymbol s) {
                        yield return s;
                    }
                }
            }
        }

        /// <summary>
        /// Gets an enumerable collection of base scopes, such as parent classes, that this scope inherits from.
        /// </summary>
        public IEnumerable<AstScope> FindBases()
        {
            if (Bases != null) {
                foreach (var b in Bases) {
                    yield return b;
                }
            }
        }

        /// <summary>
        /// Indicates whether the scope is empty (i.e., no symbols are defined in it).
        /// </summary>
        /// <returns>true if the scope is empty; otherwise, false.</returns>
        public bool IsEmpty()
        {
            return symbols == null || symbols.Count == 0;
        }

        /// <summary>
        /// Adds an <see cref="AstSymbol"/> to the scope.
        /// </summary>
        /// <param name="symbol">The symbol to add.</param>
        /// <param name="allowReplace">If true and a symbol with the same name and type is in the scope, replaces it.</param>
        internal void Add<T>(AstSymbol symbol, bool allowReplace = false) where T : AstSymbol
        {
            symbols ??= new Dictionary<(string, Type), Bag<AstSymbol>>();
            if (symbols.TryGetValue((symbol.Name, typeof(T)), out _) && !allowReplace) {
                return;
            }
            symbols[(symbol.Name, typeof(T))] = new Bag<AstSymbol>(symbol);
        }

        internal void AddOverload<T>(SymOverloadFunc symbol) where T : SymOverloadFunc
        {
            symbols ??= new Dictionary<(string, Type), Bag<AstSymbol>>();
            if (symbols.ContainsKey((symbol.Name, typeof(T)))) {
                var s = symbols[(symbol.Name, typeof(T))];
                s.Add(symbol);
                symbols[(symbol.Name, typeof(T))] = s;
            }
            else {
                symbols[(symbol.Name, typeof(T))] = new Bag<AstSymbol>(symbol);
            }
        }

        /// <summary>
        /// Gets a symbol that matches this node.
        /// </summary>
        /// <typeparam name="T">The type of the symbol to look for.</typeparam>
        /// <param name="id">The node to match.</param>
        /// <returns>The symbol, if it was found; null otherwise.</returns>
        [PythonApiInfo(newName: "FindT")]
        public AstSymbol Find<T>(AstNodeId id) where T : AstSymbol
        {
            if (symbols != null && symbols.TryGetValue((id.Name, typeof(T)), out var symbolBag)) {
                return symbolBag.Value as T;
            }
            return null;
        }

        /// <summary>
        /// Gets a symbol that matches this name and symbol type.
        /// </summary>
        /// <typeparam name="T">The type of the symbol to look for.</typeparam>
        /// <param name="name">The name to match.</param>
        /// <returns>The symbol, if it was found; null otherwise.</returns>
        [PythonApiInfo(newName: "FindTName")]
        public T Find<T>(string name) where T : AstSymbol
        {
            if (symbols != null && symbols.TryGetValue((name, typeof(T)), out var symbolBag)) {
                return symbolBag.Value as T;
            }
            return null;
        }

        /// <summary>
        /// Gets a symbol that matches this name and symbol type.
        /// </summary>
        /// <typeparam name="T">The type of the symbol to look for.</typeparam>
        /// <param name="name">The name to match.</param>
        /// <returns>The symbol, if it was found; null otherwise.</returns>
        [PythonApiInfo(newName: "FindTAll")]
        public List<AstSymbol> FindAll<T>(string name) where T : SymOverloadFunc
        {
            if (symbols != null && symbols.TryGetValue((name, typeof(T)), out var symbolBag)) {
                if (symbolBag.Value is T tValue) {
                    return [tValue];
                }
                return symbolBag.List;
            }
            return null;
        }

        /// <summary>
        /// Tries to add a new variable to this scope.
        /// If a variable of the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymField"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymField"/> creation.</param>
        /// <param name="parentSymbol">The symbol the new symbol is a field of. Null if none is passed.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewField(AstNodeId id, AstSymbol parentSymbol = null)
        {
            id.UpgradeIdentifierToField();
            if (Find<SymField>(id) is SymField sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymField(this, id, parentSymbol);
            Add<SymField>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Adds a new symbol representing the self referencing symbol on classes, structs etc.
        /// </summary>
        /// <param name="name">The name of the self referencing symbol. (Examples: this, self)</param>
        /// <param name="type">The type of the referencing symbol, which is also its parent symbol.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewSelfRef(string name, AstSymbol type)
        {
            if (Find<SymField>(name) is SymField) {
                return false;
            }

            var selfSym = new SymField(this, AstToken.None, name, type);
            Add<SymField>(selfSym);
            selfSym.InitializeTypeOf(type);
            return true;
        }

        /// <summary>
        /// Tries to add a new function to this scope.
        /// If a function with the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymFunc"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymFunc"/> creation.</param>
        /// <param name="scopeNode">The scope node that should be used to create the scope for the new symbol.</param>
        /// <param name="parentSymbol">The symbol the new symbol is a field of. Null if none is passed.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewFunc(AstNodeId id, AstNodeScope scopeNode, AstSymbol parentSymbol = null)
        {
            id.UpgradeIdentifierToDef();
            if (Find<SymFunc>(id) is SymFunc asf) {
                id.Symbol = asf;
                return false;
            }

            id.Symbol = new SymFunc(this, id, scopeNode, parentSymbol);
            Add<SymFunc>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Tries to add a new label to this scope.
        /// If a label with the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymLabel"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymLabel"/> creation.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewLabel(AstNodeId id)
        {
            id.UpgradeIdentifierToDef();
            if (Find<SymLabel>(id) is SymLabel sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymLabel(this, id);
            Add<SymLabel>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Tries to add a new module to this scope.
        /// If a module with the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymModule"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymFunc"/> creation.</param>
        /// <param name="scopeNode">The scope node that should be used to create the scope for the new symbol.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewModule(AstNodeId id, AstNodeScope scopeNode)
        {
            id.UpgradeIdentifierToDef();
            if (Find<SymModule>(id) is SymModule sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymModule(this, id, scopeNode);
            if (scopeNode != null) {
                scopeNode.Scope = id.Symbol.Scope;
            }
            Add<SymModule>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Tries to add a new named type to this scope.
        /// If a named type with the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymNamedType"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymFunc"/> creation.</param>
        /// <param name="parentSymbol">The symbol the new symbol is a field of. Null if none is passed.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewNamedType(AstNodeId id, AstSymbol parentSymbol = null)
        {
            id.UpgradeIdentifierToType();
            if (Find<SymType>(id) is SymType sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymNamedType(this, id, parentSymbol);
            Add<SymType>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Tries to add a new named type to this scope.
        /// If a named type with the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymNamedType"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymFunc"/> creation.</param>
        /// <param name="scopeNode">The scope node that should be used to create the scope for the new symbol.</param>
        /// <param name="parentSymbol">The symbol the new symbol is a field of. Null if none is passed.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewNamedType(AstNodeId id, AstNodeScope scopeNode, AstSymbol parentSymbol = null)
        {
            id.UpgradeIdentifierToType();
            if (Find<SymType>(id) is SymType sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymNamedType(this, id, scopeNode, parentSymbol);
            scopeNode.Scope = id.Symbol.Scope;
            Add<SymType>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Tries to add a new preprocessor directive to this scope.
        /// If a directive of the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymPreproc"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymPreproc"/> creation.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewPreproc(AstNodeId id)
        {
            id.UpgradeIdentifierToDef();
            if (Find<SymPreproc>(id) is SymPreproc sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymPreproc(this, id);
            Add<SymPreproc>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Tries to add a new typedef to this scope.
        /// If a typedef of the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymTypedef"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymPreproc"/> creation.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewTypedef(AstNodeId id)
        {
            id.UpgradeIdentifierToType();
            if (Find<SymType>(id) is SymType sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymTypedef(this, id);
            Add<SymType>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Tries to add a new variable to this scope.
        /// If a variable of the same name already exists, use that symbol instead.
        /// Assigns the <see cref="SymVar"/> to the provided <see cref="AstNodeId"/> symbol.
        /// </summary>
        /// <param name="id">The <see cref="AstNodeId"/> to be used for the <see cref="SymVar"/> creation.</param>
        /// <returns>True if a symbol was created; false if the symbol already exists.</returns>
        internal bool NewVar(AstNodeId id)
        {
            id.UpgradeIdentifierToDef();
            if (Find<SymVar>(id) is SymVar sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymVar(this, id);
            Add<SymVar>(id.Symbol);
            return true;
        }

        /// <summary>
        /// Finds an overloaded instance of a symbol within this scope.
        /// </summary>
        /// <param name="overloadSym">The overloaded base symbol.</param>
        /// <param name="mangled">The mangled name of the symbol, if applicable.</param>
        /// <returns>The symbol if found; otherwise, null.</returns>
        public AstSymbol FindMangled(AstSymbol overloadSym, string mangled)
        {
            Xred.Assert(mangled != null);

            if (overloadSym.Overloads != null) {
                foreach (var o in overloadSym.Overloads) {
                    if (o.Mangled == mangled) {
                        return o;
                    }
                }
            }
            return null;
        }

        /// <summary>
        /// Finds a symbol by name and mangled name within this scope. Mangled name must match.
        /// </summary>
        /// <param name="name">The name of the symbol to find.</param>
        /// <param name="mangled">The mangled name of the symbol.</param>
        /// <returns>The symbol matching the name, or null if no match is found.</returns>
        [PythonApiInfo(newName: "FindWithMangledName")]
        public AstSymbol Find(string name, string mangled)
        {
            if (symbols != null) {
                if (symbols.TryGetValue((name, typeof(AstSymbol)), out var symBag) && symBag.Value is AstSymbol sym) {
                    if (!sym.IsGeneric) {
                        if (sym.IsOverloaded) {
                            var symMangled = FindMangled(sym, mangled);
                            sym = symMangled ?? sym;
                            Xred.Assert(sym != null);
                        }
                        if (sym.Mangled != mangled) {
                            sym = null;
                        }
                    }

                    if (Xred.verboseLevel >= 3) {
                        Console.WriteLine("        <--{0,3} [{1}:{2}]", DebugId, sym?.Name, sym?.DebugId);
                    }
                    return sym;
                }
            }
            return null;
        }

        /// <summary>
        /// Finds a symbol by name within this scope.
        /// </summary>
        /// <param name="name">The name of the symbol to find.</param>
        /// <returns>The symbol matching the name, or null if no match is found.</returns>
        public AstSymbol Find(string name)
        {
            if (symbols != null) {
                if (symbols.TryGetValue((name, typeof(AstSymbol)), out var symBag) && symBag.Value is AstSymbol sym) {
                    if (Xred.verboseLevel >= 3) {
                        Console.WriteLine("        <--{0,3} [{1}:{2}]", DebugId, sym.Name, sym.DebugId);
                    }
                    return sym;
                }
            }
            return null;
        }

        /// <summary>
        /// Recursively finds symbols by name within this scope and its base scopes.
        /// </summary>
        /// <param name="name">The name of the symbol to find.</param>
        /// <returns>The list of found symbols.</returns>
        public List<AstSymbol> FindAllRecursive(string name)
        {
            var list = new List<AstSymbol>();
            if (symbols != null) {
                if (symbols.TryGetValue((name, typeof(AstSymbol)), out var symBag) && symBag.Value is AstSymbol sym) {
                    list.Add(sym);
                }
            }
            if (Bases != null) {
                foreach (var b in Bases) {
                    var sym = b.FindAllRecursive(name);
                    foreach (var s in sym) {
                        list.Add(s);
                    }
                }
            }
            return list;
        }

        /// <summary>
        /// Recursively finds a symbol by name and mangled name within this scope and its base scopes.
        /// </summary>
        /// <param name="name">The name of the symbol to find.</param>
        /// <param name="mangled">The mangled name of the symbol to find.</param>
        /// <returns>The symbol if found; otherwise, null.</returns>
        [PythonApiInfo(newName: "FindRecursiveWithMangledName")]
        public AstSymbol FindRecursive(String name, String mangled)
        {
            if (mangled == null) {
                mangled = name;
            }

            AstSymbol sym;
            if (symbols != null) {
                if (symbols.TryGetValue((name, typeof(AstSymbol)), out var symBag) && symBag.Value is AstSymbol found) {
                    sym = found;
                    if (sym.IsOverloaded) {
                        sym = FindMangled(sym, mangled);
                    }

                    if (Xred.verboseLevel >= 3) {
                        Console.WriteLine("        <--{0,3} [{1}:{2}]", DebugId, sym.Name, sym.DebugId);
                    }

                    if (sym?.Mangled == mangled) {
                        return sym;
                    }
                }
            }
            if (Bases != null) {
                foreach (var b in Bases) {
                    sym = b.FindRecursive(name, mangled);
                    if (sym != null) {
                        return sym;
                    }
                }
            }
            return null;
        }

        /// <summary>
        /// Recursively finds a symbol by name within this scope and its base scopes.
        /// </summary>
        /// <param name="name">The name of the symbol to find.</param>
        /// <returns>The symbol if found; otherwise, null.</returns>
        public AstSymbol FindRecursive(String name)
        {
            AstSymbol sym;
            if (symbols != null) {
                if (symbols.TryGetValue((name, typeof(AstSymbol)), out var symBag) && symBag.Value is AstSymbol found) {
                    sym = found;
                    if (Xred.verboseLevel >= 3) {
                        Console.WriteLine("        <--{0,3} [{1}:{2}]", DebugId, sym.Name, sym.DebugId);
                    }
                    return sym;
                }
            }
            if (Bases != null) {
                foreach (var b in Bases) {
                    sym = b.FindRecursive(name);
                    if (sym != null) {
                        return sym;
                    }
                }
            }
            return null;
        }

        /// <summary>
        /// Recursively finds a symbol for an <see cref="AstNodeId"/> within this scope and its base scopes.
        /// If the symbol is not found, it is added to the scope.
        /// </summary>
        /// <param name="node">The <see cref="AstNodeId"/> needing a symbol.</param>
        /// <returns>The symbol found or added.</returns>
        internal AstSymbol FindRecursiveOrAdd(AstNodeId node)
        {
            AstSymbol sym = FindRecursive(node.Name);
            if (sym == null) {
                sym = new AstSymbol(this, node.Tokenid, node.Name);
                Add(sym);
                node.Symbol = sym;
            }
            return sym;
        }

        /// <summary>
        /// Recursively finds a symbol for an <see cref="AstNodeId"/> within this scope and its base scopes.
        /// If the symbol is not found, it is added to the scope.
        /// </summary>
        /// <param name="node">The <see cref="AstNodeId"/> needing a symbol.</param>
        /// <param name="mangled">The mangled name of the symbol to find.</param>
        /// <returns>The symbol found or added.</returns>
        [PythonApiInfo(newName: "FindRecursiveOrAddWithMangledName")]
        internal AstSymbol FindRecursiveOrAdd(AstNodeId node, String mangled)
        {
            AstSymbol sym = FindRecursive(node.Name, mangled);
            if (sym == null) {
                sym = new AstSymbol(this, node.Tokenid, node.Name, mangled);
                Add(sym);
                node.Symbol = sym;
            }
            return sym;
        }

        /// <summary>
        /// Recursively finds a symbol for an <see cref="AstNodeId"/> within this scope and its base scopes.
        /// If the symbol is not found, it is added to the scope.
        /// </summary>
        /// <param name="node">The <see cref="AstNodeId"/> needing a symbol.</param>
        /// <param name="name">The name of the symbol to find.</param>
        /// <returns>The symbol found or added.</returns>
        internal AstSymbol FindOrAdd(AstNodeId node, String name = null)
        {
            if (name != null) {
                var symbol = Find(name);
                if (symbol != null) {
                    return symbol;
                }

                symbol = new AstSymbol(this, node.Tokenid, name);
                Add(symbol);
                return symbol;
            }

            if (node.Symbol == null) {
                var symbol = Find(node.Name, node.Mangled);
                if (symbol != null && (!symbol.IsOverloaded || symbol.IsGeneric)) {
                    if (Xred.verboseLevel >= 3) {
                        Console.WriteLine("        <--{0,3} [{1}:{2}]", DebugId, node.Name, symbol.DebugId);
                    }
                    return symbol;
                }
                symbol = new AstSymbol(this, node.Tokenid, node.Name, node.Mangled);
                Add(symbol);
                node.Symbol = symbol;
                return symbol;
            }
            return node.Symbol;
        }

        /// <summary>
        /// Adds or replaces a symbol in this scope for an <see cref="AstNodeId"/>.
        /// </summary>
        /// <param name="symbol">The <see cref="AstSymbol"/> to add or replace.</param>
        /// <returns>Returns the symbol.</returns>
        internal AstSymbol AddOrReplace(AstSymbol symbol)
        {
            Remove(symbol.Name);
            Add(symbol);
            return symbol;
        }

        internal AstSymbol AddOrReplace(AstNodeId node)
        {
            var symbol = node.Symbol ?? Find(node.Name);
            if (symbol != null) {
                var newSymbol = new AstSymbol(this, node.Tokenid, node.Name, node.Mangled) {
                    Previous = symbol,
                    Origin = node.Begrc
                };
                Remove(symbol.Name);
                Add(newSymbol);
                return newSymbol;
            }
            return FindOrAdd(node);
        }

        internal bool Remove(string symbolName)
        {
            if (Find(symbolName) != null) {
                symbols.Remove((symbolName, typeof(AstSymbol)));
                return true;
            }
            return false;
        }

        /// <summary>
        /// Clears all symbols from the scope.
        /// </summary>
        internal void ClearSymbols()
        {
            if (symbols != null) {
                symbols.Clear();
            }
        }

        /// <summary>
        /// Adds a symbol to this scope.
        /// </summary>
        /// <param name="symbol">The symbol to add.</param>
        internal void Add(AstSymbol symbol)
        {
            symbols ??= isCaseInsensitive
                ? new Dictionary<(string, Type), Bag<AstSymbol>>(TupleStringTypeComparer.OrdinalIgnoreCase)
                : new Dictionary<(string, Type), Bag<AstSymbol>>();
            var overlap = Find(symbol.Name);
            if (overlap == null) {
                Add<AstSymbol>(symbol);
            }
            else if (overlap.Mangled != symbol.Mangled) {
                if (!overlap.IsOverloaded) {
                    AstSymbol overload = new AstSymbol(this, symbol.Name);
                    overload.Overloads ??= [];
                    overload.Overloads.Add(overlap);
                    overload.Overloads.Add(symbol);
                    Remove(overlap.Name);
                    Add<AstSymbol>(overload);
                }
                else {
                    overlap.Overloads ??= [];
                    overlap.Overloads.Add(symbol);
                }
            }

            if (Xred.verboseLevel >= 3) {
                Console.WriteLine("        -->{0,3} [{1}:{2}]", DebugId, symbol.Name, symbol.DebugId);
            }
        }

        internal void AddThis(AstContext context, AstSymbol type, string name)
        {
            symbols ??= isCaseInsensitive
                ? new Dictionary<(string, Type), Bag<AstSymbol>>(TupleStringTypeComparer.OrdinalIgnoreCase)
                : new Dictionary<(string, Type), Bag<AstSymbol>>();
            if (!symbols.ContainsKey((name, typeof(AstSymbol)))) {
                var node = new AstNodeId(context, AstCategory.IdentifierDef, name);
                var symbol = new AstSymbol(this, node.Tokenid, name);
                symbol.TypeOf = type;
                Add(symbol);
            }
        }

        /// <summary>
        /// Adds a range of symbols to this scope.
        /// </summary>
        /// <param name="values">The symbols to add.</param>
        internal void AddRange(IEnumerable<AstSymbol> values)
        {
            symbols ??= isCaseInsensitive
                ? new Dictionary<(string, Type), Bag<AstSymbol>>(TupleStringTypeComparer.OrdinalIgnoreCase)
                : new Dictionary<(string, Type), Bag<AstSymbol>>();
            foreach (var symbol in values) {
                Add<AstSymbol>(symbol);
                if (Xred.verboseLevel >= 3) {
                    Console.WriteLine("        -->{0,3} [{1}:{2}]", DebugId, symbol.Name, symbol.DebugId);
                }
            }
        }

        /// <summary>
        /// Adds a base scope, such as a parent class, to this scope, indicating that this scope inherits from the base scope.
        /// </summary>
        /// <param name="_base">The base scope to add.</param>
        internal void AddBase(AstScope _base)
        {
            Xred.Assert(_base != null);
            Xred.Assert(_base != this); // this would loop forever
            if (Bases == null) {
                Bases = new List<AstScope>();
            }

            // Avoid adding the base scope if it is the same as the owner scope.
            if (_base.Owner != null && Owner != null) {
                Xred.Assert(_base.Owner.Scope != Owner.Scope);
            }

            // [larissar] if the base scope contains this scope it will cause a stack overflow during block linking
            // this is an issue when importing header files with different macro expansions
            if (!_base.ContainsScope(this) && this != _base && !Bases.Contains(_base)) {
                Bases.Insert(0, _base);
            }
        }

        private bool ContainsScope(AstScope other)
        {
            if (Bases != null) {
                if (Bases.Contains(other)) {
                    return true;
                }
                foreach (var b in Bases) {
                    if (b.ContainsScope(other)) {
                        return true;
                    }
                }
            }

            return false;
        }

        /// <summary>
        /// Register a primitive type to this scope.
        /// </summary>
        /// <param name="node">The node for the symbol.</param>
        internal void RegisterPrimitive(AstNodeId node)
        {
            Xred.Assert(node.Name != null && node.Name != "");
            Xred.Assert(node.Symbol != null);
            Add(node.Symbol);
        }

        internal void RegisterPrimitive(AstSymbol symbol)
        {
            Xred.Assert(symbol.Name != null && symbol.Name != "");
            Add(symbol);
            symbol.OverrideParentScope(this);
        }

        /// <summary>
        /// Prints the symbols defined in this scope to the console.
        /// </summary>
        /// <param name="indent">The indentation level for printing the symbols.</param>
        public void Print(int indent = 0)
        {
            if (Owner != null) {
                Console.WriteLine("{0}: {1} ***", Owner.Format(0), symbols != null ? symbols.Count : 0);
            }
            if (symbols != null) {
                foreach (var s in symbols.Values) {
                    if (s.Value is AstSymbol symbol) {
                        Console.Write("{0,3}            ", DebugId);
                        symbol.Print(0);
                    }
                }

            }
        }

        private class TupleStringTypeComparer : IEqualityComparer<(string, Type)>
        {
            public static readonly TupleStringTypeComparer OrdinalIgnoreCase = new TupleStringTypeComparer();

            public bool Equals((string, Type) x, (string, Type) y)
            {
                return string.Equals(x.Item1, y.Item1, StringComparison.OrdinalIgnoreCase)
                    && x.Item2 == y.Item2;
            }

            public int GetHashCode((string, Type) obj)
            {
                int hash1 = StringComparer.OrdinalIgnoreCase.GetHashCode(obj.Item1);
                int hash2 = obj.Item2?.GetHashCode() ?? 0;
                return hash1 ^ hash2;
            }
        }
    }

    internal class AstScopeUndef : AstScope
    {
        internal AstScopeUndef(AstNodeScope root)
            : base(root)
        {

        }

        internal bool NewUndefCall(AstNodeId id, AstNodeBase baseNode, List<AstSymbol> argTypes, List<AstSymbol> genTypes)
        {
            if (Find<SymUndefCall>(id) is SymUndefCall sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymUndefCall(this, id, argTypes, genTypes);
            Add<SymUndefCall>(id.Symbol);
            ((SymUndefCall)id.Symbol).AddNode(baseNode);
            return true;
        }

        internal bool NewUndefLabel(AstNodeId id, AstNodeBase baseNode)
        {
            if (Find<SymUndefLabel>(id) is SymUndefLabel sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymUndefLabel(this, id);
            Add<SymUndefLabel>(id.Symbol);
            ((SymUndefLabel)id.Symbol).AddNode(baseNode);
            return true;
        }

        internal bool NewUndefModule(AstNodeId id, AstNodeBase baseNode)
        {
            if (Find<SymUndefModule>(id) is SymUndefModule sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymUndefModule(this, id);
            Add<SymUndefModule>(id.Symbol);
            ((SymUndefModule)id.Symbol).AddNode(baseNode);
            return true;
        }

        internal bool NewUndefType(AstNodeId id, AstNodeBase baseNode)
        {
            if (Find<SymUndefType>(id) is SymUndefType sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymUndefType(this, id);
            Add<SymUndefType>(id.Symbol);
            ((SymUndefType)id.Symbol).AddNode(baseNode);
            return true;
        }

        internal bool NewUndefVar(AstNodeId id, AstNodeBase baseNode)
        {
            if (Find<SymUndefVar>(id) is SymUndefVar sym) {
                id.Symbol = sym;
                return false;
            }

            id.Symbol = new SymUndefVar(this, id);
            Add<SymUndefVar>(id.Symbol);
            ((SymUndefVar)id.Symbol).AddNode(baseNode);
            return true;
        }
    }

    internal class AstScopeStack
    {
        private List<AstScope> scopes;

        internal IReadOnlyList<AstScope> Scopes { get => scopes; }

        public int Count { get { return scopes.Count; } }
        public AstScope Top { get { return scopes[scopes.Count - 1]; } }

        public AstScopeStack()
        {
            scopes = [];
        }

        /// <summary>
        /// Returns a string that represents the current <see cref="AstScopeStack"/>. Format is optimized for internal debugging.
        /// </summary>
        /// <returns>A string that represents the current <see cref="AstScopeStack"/>.</returns>
        public override string ToString()
        {
            return String.Format("AstScopeStack count={0}", Count);
        }

        public AstScope PushToParentIfAny(AstScope scope)
        {
            Xred.Assert(scope != null);
            if (scopes.Count > 1) {
                scopes.Insert(scopes.Count - 1, scope);
            }
            else {
                scopes.Add(scope);
            }
            return scope;
        }

        public AstScope Push(AstScope scope)
        {
            Xred.Assert(scope != null);
            scopes.Add(scope);
            if (Xred.verboseLevel >= 3) {
                var sb = new StringBuilder();
                sb.Append('-', scopes.Count);
                sb.AppendFormat("{0,3} push {1}", scope.DebugId, scope.Owner != null ? "[" + scope.Owner.Name + ":" + scope.Owner.DebugId + "]" : "");
                Console.WriteLine(sb.ToString());
                for (int s = 0; s < scopes.Count - 1; s++) {
                    if (scopes[s] == scope) {
                        Xred.Assert(scopes[s] != scope);
                    }
                }
            }
            return scope;
        }

        public AstScope Push(AstScope scope, bool recursive)
        {
            // The code below effectively replaces the recursive Push function with an iterative approach using stacks.
            // By using two stacks, it maintains the correct processing order: scopesToProcess ensures that all base
            // scopes are processed before their parent scopes. scopesToPush stores the scopes in reverse order, allowing
            // them to be pushed in the correct order when popped and processed.
            if (!recursive) {
                return Push(scope);
            }
            var scopesToPush = new List<AstScope>();
            var scopesToProcess = new Stack<AstScope>();

            scopesToProcess.Push(scope);

            while (scopesToProcess.Count > 0) {
                AstScope currentScope = scopesToProcess.Pop();
                scopesToPush.Add(currentScope);

                if (currentScope.Bases != null) {
                    foreach (var b in currentScope.Bases) {
                        bool found = false;
                        for (int s = 0; s < scopesToPush.Count; s++) {
                            if (scopesToPush[s] == b) {
                                found = true;
                                break;
                            }
                        }
                        if (!found) {
                            scopesToProcess.Push(b);
                        }
                    }
                }
            }

            // Push the scopes in the reverse order.
            for (int i = scopesToPush.Count - 1; i >= 0; i--) {
                Push(scopesToPush[i]);
            }
            return scope;
        }

        public AstScope Pop()
        {
            Xred.Assert(scopes.Count > 0);
            var scope = scopes[scopes.Count - 1];
            if (Xred.verboseLevel >= 3) {
                var sb = new StringBuilder();
                sb.Append('-', scopes.Count);
                sb.AppendFormat("{0,3} pop {1}", scope.DebugId, scope.Owner != null ? "[" + scope.Owner.Name + "]" : "");
                Console.WriteLine(sb.ToString());
            }
            scopes.RemoveAt(scopes.Count - 1);
            return scope;
        }

        public void PopUntil(int depth)
        {
            Xred.Assert(scopes.Count >= depth);

            if (depth < scopes.Count) {
                if (Xred.verboseLevel >= 3) {
                    for (int n = scopes.Count - 1; n >= depth; n--) {
                        var sb = new StringBuilder();
                        sb.Append('-', n + 1);
                        sb.AppendFormat("{0,3} pop {1}", scopes[n].DebugId, scopes[n].Owner != null ? "[" + scopes[n].Owner.Name + "]" : "");
                        Console.WriteLine(sb.ToString());
                    }
                }
                scopes.RemoveRange(depth, scopes.Count - depth);
            }
        }

        public AstSymbol Find(string name, bool parent = false, bool top = false, bool global = false, string mangled = null)
        {
            Xred.Assert(name != null && name != "");
            if (name.StartsWith(':')) {
                if (scopes.Count > 0) {
                    if (mangled == null) {
                        return scopes[0].Find(name);
                    }
                    else {
                        return scopes[0].Find(name, mangled);
                    }
                }
                return null;
            }
            else if (parent) {
                if (mangled == null) {
                    return scopes[scopes.Count - 2].Find(name);
                }
                else {
                    return scopes[scopes.Count - 2].Find(name, mangled);
                }
            }
            else if (top) {
                if (mangled == null) {
                    return scopes[scopes.Count - 1].Find(name);
                }
                else {
                    return scopes[scopes.Count - 1].Find(name, mangled);
                }
            }
            else if (global) {
                if (mangled == null) {
                    return scopes[0].Find(name);
                }
                else {
                    return scopes[0].Find(name, mangled);
                }

            }
            for (int s = scopes.Count - 1; s >= 0; s--) {
                AstSymbol id = null;
                if (mangled == null) {
                    id = scopes[s].Find(name);
                }
                else {
                    id = scopes[s].Find(name, mangled);
                }

                if (id != null) {
                    return id;
                }
            }
            return null;
        }

        internal T Find<T>(AstNodeId id) where T : AstSymbol
        {
            for (int s = scopes.Count - 1; s >= 0; s--) {
                if (scopes[s].Find<T>(id) is T found) {
                    return found;
                }
            }
            return null;
        }

        internal T Find<T>(string name) where T : AstSymbol
        {
            for (int s = scopes.Count - 1; s >= 0; s--) {
                if (scopes[s].Find<T>(name) is T found) {
                    return found;
                }
            }
            return null;
        }

        internal IEnumerable<List<AstSymbol>> FindOverloads<T>(string name) where T : SymOverloadFunc
        {
            for (int s = scopes.Count - 1; s >= 0; s--) {
                var symbols = scopes[s].FindAll<T>(name);
                if (symbols != null) {
                    yield return symbols;
                }
            }
        }

        public AstSymbol FindOrAddAnonymous(AstNodeId node, AstNodeBase defined, bool global = false, bool parent = false)
        {
            AstScope scope;
            if (global) {
                scope = scopes[0];
            }
            else if (parent) {
                scope = scopes[scopes.Count - 2];
            }
            else {
                scope = scopes[scopes.Count - 1];
            }

            if (FindAnonymous(scope.Find(""), defined) is AstSymbol sym) {
                node.Symbol = sym;
            }
            else {
                node.Symbol = new AstSymbol(scope, node.Tokenid, "", idAsMangled: true);
                scope.Add(node.Symbol);
            }

            return node.Symbol;
        }

        private AstSymbol FindAnonymous(AstSymbol sym, AstNodeBase defined)
        {
            if (sym == null) {
                return null;
            }

            if (sym.IsOverloaded) {
                foreach (var child in sym.Overloads) {
                    if (child.Defined == defined) {
                        return child;
                    }
                }
                return null;
            }
            else {
                return sym.Defined == defined ? sym : null;
            }
        }

        public AstSymbol FindOrAdd(AstNodeId node, bool global = false, bool parent = false, bool top = false, bool force = false, bool useMangledName = false)
        {
            if (node.Name == null || node.Name == "") {
                if (force) {
                    node.Symbol = _FindOrAdd(node, "___anonymous___", global, parent, top);
                    return node.Symbol;
                }
                return null;    // Handle placeholders.
            }
            if (node.Symbol == null) {
                node.Symbol = _FindOrAdd(node, node.Name, global, parent, top, useMangledName);
            }
            return node.Symbol;

            AstSymbol _FindOrAdd(AstNodeId node, string name, bool global = false, bool parent = false, bool top = false, bool useMangledName = false)
            {
                AstSymbol symbol;
                AstScope scope = null;
                if (scopes.Count > 0) {
                    if (global || scopes.Count == 1) {
                        scope = scopes[0];
                    }
                    else if (parent && scopes.Count >= 2) {
                        scope = scopes[scopes.Count - 2];
                    }
                    else {
                        scope = scopes[scopes.Count - 1];
                    }
                }
                if (useMangledName) {
                    symbol = Find(name, parent: parent, top: top, mangled: node.Mangled);
                    if (symbol != null && (symbol.Mangled == node.Mangled || symbol.IsGeneric)) {
                        while (symbol.Previous != null && symbol.Origin > node.Begrc) {
                            symbol = symbol.Previous;
                        }
                        return symbol;
                    }
                    symbol = new AstSymbol(scope, node.Tokenid, name, node.Mangled) {
                        Origin = node.Begrc
                    };
                }
                else {
                    symbol = Find(name, parent: parent, top: top);
                    if (symbol != null) {
                        while (symbol.Previous != null && symbol.Origin > node.Begrc) {
                            symbol = symbol.Previous;
                        }
                        return symbol;
                    }
                    symbol = new AstSymbol(scope, node.Tokenid, name) {
                        Origin = node.Begrc
                    };
                }

                if (scope != null) {
                    scope.Add(symbol);
                }
                return symbol;
            }
        }

        public List<AstSymbol> FindAll(string name)
        {
            var symbolList = new List<AstSymbol>();
            for (int s = scopes.Count - 1; s >= 0; s--) {
                var sym = scopes[s].FindRecursive(name);
                if (sym != null) {
                    symbolList.Add(sym);
                }
            }
            return symbolList;
        }

        public bool Remove(AstSymbol symbol, bool parent = false)
        {
            if (parent) {
                return scopes[scopes.Count - 2].Remove(symbol.Name);
            }
            else {
                return scopes[scopes.Count - 1].Remove(symbol.Name);
            }
        }

        public void Print(int indent = 0)
        {
            for (int s = scopes.Count - 1; s >= 0; s--) {
                scopes[s].Print(indent + s * 2);
            }
        }

        public void AddToFirstNamed(AstNodeId node)
        {
            var s = Top;
            for (int i = scopes.Count - 1; i >= 0; i--) {
                if (s.Owner?.Name != "") {
                    break;
                }
                s = scopes[i];
            }
            s.Add(node.Symbol);
        }
    }
}
