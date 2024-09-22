import unittest

from llmcc.parser import parse_doc
from llmcc.compiler import compile_graph
from llmcc.printer import write_graph, print_graph
from llmcc.slicer import *
from llmcc.analyzer import analysis_graph


class TestSlicer(unittest.TestCase):

    @parse_doc()
    def test_query(self, g):
        """
        namespace Slicer {
            enum Color {RED, BLACK, DOUBLE_BLACK};
            class Foo {
                static const int var = 2;
                // define a
                int *a;
                Color b;
                int c;
                void * f = int(a, b);

                // function declarator
                void bar();

                int another_func(int c) {
                    return c + 2;
                }

                inline int sum() {
                    Color e;
                    Bar w;
                    return e + another_func(c) + 2;
                }

                class Bar {
                    Color d;
                    int e;

                    int bzz() {
                        return d + 2;
                    }
                };

            };

            class ABC {
                int a, b, c;
            };

            void Foo::bar() {
                printf("hello");
            }
        }

        class DCE {
            int d, c, e;
        };
        """
        print_graph(g)
        slice_graph(g)
        analysis_graph(g)
        for k, v in g.node_map.items():
            print(k, v)
        # slice_graph(g)


if __name__ == "__main__":
    unittest.main()
